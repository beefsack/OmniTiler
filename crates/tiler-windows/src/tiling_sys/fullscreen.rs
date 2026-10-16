use super::*;

/// Style bits the project-owned fullscreen toggle clears on entry
/// (`WS_CAPTION` covers border+frame, `WS_THICKFRAME` the sizing border).
/// Only these bits are ever stored and restored; unrelated style changes the
/// application makes meanwhile are preserved.
pub(super) const FULLSCREEN_STYLE_BITS: u32 = WS_CAPTION | WS_THICKFRAME;
/// Inert per-window ownership marker for the project-owned fullscreen toggle.
/// Nonzero magic only; never a pointer, never trusted across window lifetime
/// (the property dies with its window, so a recycled HWND never inherits it).
const FULLSCREEN_MARKER: usize = 0x4653_3131;
/// Ownership marker property: presence of the exact magic proves a
/// project-owned fullscreen frame with restoration metadata alongside.
const FULLSCREEN_PROP: &str = "OmniTilerFullscreen";
/// Stored changed style bits plus one (never zero when present, so absence
/// stays distinguishable from a no-op clear).
const FULLSCREEN_STYLE_PROP: &str = "OmniTilerFullscreenStyle";
/// Preexisting maximize state: 1 was normal, 2 was maximized. Absence means
/// corrupt metadata, never a default.
const FULLSCREEN_MAX_PROP: &str = "OmniTilerFullscreenMax";

fn prop_wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain([0]).collect()
}

/// Owned restoration metadata for one project-toggled fullscreen window: the
/// exact style bits cleared on entry plus whether the window was maximized
/// before (fullscreen wins over maximize; exit reasserts it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FullscreenMeta {
    pub(super) changed: u32,
    pub(super) was_maximized: bool,
}

/// Read the owned fullscreen metadata. `None` means app-owned or absent:
/// a missing marker, a wrong magic, any missing companion property, or a
/// changed-bits value outside the owned mask all read as unowned, never as
/// a default preimage. Only bits in `FULLSCREEN_STYLE_BITS` are ever owned;
/// arbitrary style bits are never trusted for restoration.
pub(super) fn read_fullscreen_meta(hwnd_u64: u64) -> Option<FullscreenMeta> {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    let marker = unsafe { GetPropW(hwnd, prop_wide(FULLSCREEN_PROP).as_ptr()) };
    if marker.is_null() || marker as usize != FULLSCREEN_MARKER {
        return None;
    }
    let bits = unsafe { GetPropW(hwnd, prop_wide(FULLSCREEN_STYLE_PROP).as_ptr()) };
    if bits.is_null() {
        return None;
    }
    let changed = u32::try_from((bits as usize).checked_sub(1)?).ok()?;
    if changed & !FULLSCREEN_STYLE_BITS != 0 {
        return None;
    }
    let max = unsafe { GetPropW(hwnd, prop_wide(FULLSCREEN_MAX_PROP).as_ptr()) };
    if max.is_null() {
        return None;
    }
    let was_maximized = match max as usize {
        1 => false,
        2 => true,
        _ => return None,
    };
    Some(FullscreenMeta {
        changed,
        was_maximized,
    })
}

/// Remove one owned metadata property set best-effort. Used to clean partial
/// `SetProp` writes so a failed entry never leaves a trusted-looking residue.
fn remove_fullscreen_meta_props(hwnd: HWND) {
    for name in [FULLSCREEN_PROP, FULLSCREEN_STYLE_PROP, FULLSCREEN_MAX_PROP] {
        unsafe {
            RemovePropW(hwnd, prop_wide(name).as_ptr());
        }
    }
}

/// Store the owned metadata with readback: all three properties must read
/// back exactly, or the entry fails closed with no partial residue trusted.
/// A partial `SetProp` sequence cleans up before returning false, so a later
/// press never mistakes residue for an owned preimage.
fn write_fullscreen_meta(hwnd_u64: u64, meta: &FullscreenMeta) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    let stored_bits = (meta.changed as usize).wrapping_add(1);
    let stored_max = if meta.was_maximized { 2usize } else { 1usize };
    if unsafe {
        SetPropW(
            hwnd,
            prop_wide(FULLSCREEN_PROP).as_ptr(),
            FULLSCREEN_MARKER as _,
        )
    } == 0
    {
        return false;
    }
    if unsafe {
        SetPropW(
            hwnd,
            prop_wide(FULLSCREEN_STYLE_PROP).as_ptr(),
            stored_bits as _,
        )
    } == 0
    {
        remove_fullscreen_meta_props(hwnd);
        return false;
    }
    if unsafe {
        SetPropW(
            hwnd,
            prop_wide(FULLSCREEN_MAX_PROP).as_ptr(),
            stored_max as _,
        )
    } == 0
    {
        remove_fullscreen_meta_props(hwnd);
        return false;
    }
    if read_fullscreen_meta(hwnd_u64).is_some_and(|back| back == *meta) {
        true
    } else {
        remove_fullscreen_meta_props(hwnd);
        false
    }
}

/// Remove the owned metadata after a verified restore. Best-effort removal
/// with an absence readback; a failed removal keeps the marker so a later
/// exit still finds its preimage rather than stranding the frame.
pub(super) fn clear_fullscreen_meta(hwnd_u64: u64) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    for name in [FULLSCREEN_PROP, FULLSCREEN_STYLE_PROP, FULLSCREEN_MAX_PROP] {
        unsafe {
            RemovePropW(hwnd, prop_wide(name).as_ptr());
        }
    }
    read_fullscreen_meta(hwnd_u64).is_none()
}

/// Live style read for stale-metadata retirement: `None` when the window is
/// dead or the style word is unreadable (fail closed, never guessed).
pub(super) fn live_style_for_retire(hwnd_u64: u64) -> Option<u32> {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    unsafe {
        SetLastError(0);
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let style_err = unsafe { GetLastError() };
    if style == 0 && style_err != 0 {
        return None;
    }
    Some(style)
}

/// Shared stale-retirement ordering for owned fullscreen markers (user
/// decision 2026-10-12): the single production sequence used by the
/// directional move-clear, the B9 unfloat-clear, and the reconcile sweep, so
/// no special action is needed to trigger it. A stale marker whose chrome
/// provably restored by other means retires here (metadata removal only,
/// never a geometry write); every other shape keeps its marker so the veto
/// below still holds.
///
/// Sequence: no work without a pre-read marker or when the caller's
/// home/focus/suspend/elevation guards already fail (no mutation then,
/// preserving scope and game safety); quick pid fence; first live
/// restoration proof (style contains all `changed` plus live no-cover);
/// immediately-before-removal exact revalidation (member key, window tag,
/// held process/identity, scope/hosted/proof via the existing target
/// fences); re-read metadata must equal the probed preimage; second live
/// proof must still hold; pid still current; only then best-effort removal
/// with an absence re-read. Any unreadable leg, drift, re-fullscreen, tag
/// change, same-PID reuse, zero-bit preimage, or failed removal keeps the
/// marker for a later press. No resurrection of incomplete preimages (a
/// missing companion never reads as a preimage), no writes whatsoever on
/// fullscreen/transition.
#[allow(clippy::too_many_arguments)]
pub(super) fn effective_stale_meta_via_effects(
    effects: &mut dyn UnfloatClearEffects,
    state: &mut TileLoop,
    member_key: &crate::workspace::WindowKey,
    expected: &ObservedWindow,
    me: &ProcessIdentity,
    meta: Option<FullscreenMeta>,
    guards_ok: bool,
    fulls: &[Rect],
) -> Option<FullscreenMeta> {
    let pre = meta?;
    if !guards_ok {
        return Some(pre);
    }
    if !effects.pid_current(member_key.hwnd, expected.identity.pid) {
        return Some(pre);
    }
    let (style_opt, fullscreen_opt) = effects.retire_probe(member_key.hwnd, fulls);
    let restored = style_opt.is_some_and(|live| live & pre.changed == pre.changed);
    if !crate::tiling::stale_fullscreen_retire_eligible(pre.changed, restored, fullscreen_opt) {
        return Some(pre);
    }
    // Immediately-before-removal lifetime pin: member key, window tag, held
    // process/identity, scope/hosted/proof. Any drift fails closed.
    let held = match effects.revalidate(state, member_key, expected, me, fulls) {
        Ok(target) => target.held,
        Err(_) => return Some(pre),
    };
    let _ = &held;
    // The marker must still equal the probed preimage: a changed marker
    // (including a genuine re-fullscreen) never deletes another preimage.
    // An externally vanished marker reads absent and stays absent.
    let cur = effects.fullscreen_meta(member_key.hwnd);
    if cur != Some(pre) {
        return cur;
    }
    // The restoration proof must still be live after the pin.
    let (style2, fullscreen2) = effects.retire_probe(member_key.hwnd, fulls);
    let restored2 = style2.is_some_and(|live| live & pre.changed == pre.changed);
    if !crate::tiling::stale_fullscreen_retire_eligible(pre.changed, restored2, fullscreen2) {
        return Some(pre);
    }
    if !effects.pid_current(member_key.hwnd, expected.identity.pid) {
        return effects.fullscreen_meta(member_key.hwnd).or(Some(pre));
    }
    if effects.retire_clear(member_key.hwnd) {
        effects.fullscreen_meta(member_key.hwnd)
    } else {
        effects.fullscreen_meta(member_key.hwnd).or(Some(pre))
    }
}

/// Enter project-owned fullscreen: store the restoration preimage first, then
/// clear the frame bits and cover the monitor without activating. No blind
/// input synthesis; the official style/frame APIs only. Re-entry never
/// overwrites an existing preimage: an owned frame always exits first, so an
/// entry that finds metadata returns without a write. Returns the
/// readback-settled outcome: `fullscreen` when the frame bits read back
/// cleared, `dispatched` when a setter accepted but completion is pending or
/// uncertain (including a `SetWindowPos` failure after the chrome changed,
/// where the preimage is preserved for a later exit), `threw` when no style
/// effect happened (the fresh preimage is cleared, nothing is left trusted).
/// No automatic retries on any path.
pub(super) fn enter_fullscreen(hwnd_u64: u64, full: Rect) -> &'static str {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return "threw";
    }
    if read_fullscreen_meta(hwnd_u64).is_some() {
        return "dispatched";
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let meta = FullscreenMeta {
        changed: style & FULLSCREEN_STYLE_BITS,
        was_maximized: is_zoomed_now(hwnd_u64),
    };
    if !write_fullscreen_meta(hwnd_u64, &meta) {
        return "threw";
    }
    let next = (style & !FULLSCREEN_STYLE_BITS) as i32;
    unsafe {
        SetLastError(0);
    }
    let prev = unsafe { SetWindowLongW(hwnd, GWL_STYLE, next) };
    let style_err = unsafe { GetLastError() };
    if prev == 0 && style_err != 0 {
        clear_fullscreen_meta(hwnd_u64);
        return "threw";
    }
    let placed = unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            full.x,
            full.y,
            full.w,
            full.h,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED,
        )
    };
    if placed == 0 {
        return "dispatched";
    }
    if unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32 & FULLSCREEN_STYLE_BITS == 0 {
        "fullscreen"
    } else {
        "dispatched"
    }
}

/// Exit project-owned fullscreen: restore exactly the owned mask bits
/// (unrelated application style changes meanwhile are preserved), reapply
/// the frame without moving (so externally moved frames still exit), reassert
/// a preexisting maximize, then drop the metadata. A failed restore keeps
/// the metadata so a later press can retry; nothing is guessed and nothing
/// retries automatically. `meta.changed` is already validated to the owned
/// mask on read, so restoration never touches arbitrary bits.
pub(super) fn exit_fullscreen_owned(hwnd_u64: u64, meta: &FullscreenMeta) -> &'static str {
    exit_fullscreen_owned_with(&mut NativeOwnedExit, hwnd_u64, meta)
}

/// Native effects behind the project-owned fullscreen exit, split from the
/// effect ordering so hermetic tests can substitute scripted answers while
/// the production sequence (style restore, frame reapply, readback verify,
/// maximize reassert, metadata removal) runs unchanged. [`NativeOwnedExit`]
/// calls exactly the functions the inline code called; behavior is identical.
/// Message-only owned windows cannot carry this path (never enumerated with
/// live frames), so scripted stubs are the hermetic route.
pub(super) trait OwnedExitEffects {
    fn is_window(&mut self, hwnd: u64) -> bool;
    fn current_style(&mut self, hwnd: u64) -> u32;
    /// Exactly like `SetWindowLongW` plus `GetLastError`: `(prev, err)`.
    fn restore_style(&mut self, hwnd: u64, next: i32) -> (i32, u32);
    fn reapply_frame(&mut self, hwnd: u64) -> bool;
    fn style_readback(&mut self, hwnd: u64) -> u32;
    fn reassert_maximize(&mut self, hwnd: u64) -> bool;
    fn clear_meta(&mut self, hwnd: u64) -> bool;
}

/// Production native effects for the owned exit: exact pass-throughs to the
/// Win32 reads/writes the route requires.
struct NativeOwnedExit;

impl OwnedExitEffects for NativeOwnedExit {
    fn is_window(&mut self, hwnd: u64) -> bool {
        let hwnd = hwnd as isize as HWND;
        let live = unsafe { IsWindow(hwnd) };
        live != 0
    }

    fn current_style(&mut self, hwnd: u64) -> u32 {
        let style = unsafe { GetWindowLongW(hwnd as isize as HWND, GWL_STYLE) };
        style as u32
    }

    fn restore_style(&mut self, hwnd: u64, next: i32) -> (i32, u32) {
        unsafe {
            SetLastError(0);
        }
        let prev = unsafe { SetWindowLongW(hwnd as isize as HWND, GWL_STYLE, next) };
        let err = unsafe { GetLastError() };
        (prev, err)
    }

    fn reapply_frame(&mut self, hwnd: u64) -> bool {
        let placed = unsafe {
            SetWindowPos(
                hwnd as isize as HWND,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        };
        placed != 0
    }

    fn style_readback(&mut self, hwnd: u64) -> u32 {
        let style = unsafe { GetWindowLongW(hwnd as isize as HWND, GWL_STYLE) };
        style as u32
    }

    fn reassert_maximize(&mut self, hwnd: u64) -> bool {
        let shown = unsafe { ShowWindowAsync(hwnd as isize as HWND, SW_MAXIMIZE) };
        shown != 0
    }

    fn clear_meta(&mut self, hwnd: u64) -> bool {
        clear_fullscreen_meta(hwnd)
    }
}

/// Owned-exit effect ordering shared by the native wrapper and hermetic
/// tests: metadata is removed only after the style restore, frame reapply,
/// style readback, and maximize reassert all verify. Any failure keeps the
/// metadata so a later explicit press can retry.
pub(super) fn exit_fullscreen_owned_with(
    effects: &mut dyn OwnedExitEffects,
    hwnd_u64: u64,
    meta: &FullscreenMeta,
) -> &'static str {
    if !effects.is_window(hwnd_u64) {
        return "threw";
    }
    let current = effects.current_style(hwnd_u64);
    let next = (current | meta.changed) as i32;
    let (prev, style_err) = effects.restore_style(hwnd_u64, next);
    if prev == 0 && style_err != 0 {
        return "threw";
    }
    if !effects.reapply_frame(hwnd_u64) {
        return "threw";
    }
    if effects.style_readback(hwnd_u64) & meta.changed != meta.changed {
        return "dispatched";
    }
    if meta.was_maximized && !effects.reassert_maximize(hwnd_u64) {
        return "dispatched";
    }
    if effects.clear_meta(hwnd_u64) {
        "restored"
    } else {
        "dispatched"
    }
}

/// Fresh holdability check for one fullscreen HWND: same-session,
/// medium-integrity identity plus every safety gate observation admission
/// requires (never elevated, shell, tool, owned, dialog including the own
/// Settings UI, cloaked, or no-activate; in scope with a live hosted child where listed; frozen
/// allowlist plus owned-helper verification in proof modes). Returns the
/// member key on pass; the caller decides hold vs exemption. Read-only.
pub(super) fn holdable_key(
    state: &TileLoop,
    me: &ProcessIdentity,
    hwnd_u64: u64,
) -> Option<crate::workspace::WindowKey> {
    if hwnd_u64 == 0 {
        return None;
    }
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == 0 || pid == me.pid {
        return None;
    }
    let held = HeldProcess::open(pid).ok()?;
    let live = held.identity().ok()?;
    if live.pid != pid {
        return None;
    }
    if live.user_sid != me.user_sid || live.session_id != me.session_id {
        return None;
    }
    if held.integrity().ok().is_none_or(|rid| !is_medium_rid(rid)) {
        return None;
    }
    if unsafe { IsIconic(hwnd) } != 0 {
        return None;
    }
    let class = class_of(hwnd);
    // Own Settings is an ordinary slotless member (never a dialog);
    // generic unowned `#32770` dialogs still refuse here while owned
    // windows (including owned dialogs) refuse at the owned gate below.
    let owned_live = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    if is_shell_class(&class)
        || crate::tiling::workspace_dialog_refuses(&class, owned_live, &live.exe_path, &me.exe_path)
    {
        return None;
    }
    if !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null() {
        return None;
    }
    let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    if exstyle & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) != 0 {
        return None;
    }
    let mut cloaked: i32 = 0;
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } != 0
    {
        return None;
    }
    if cloaked != 0 {
        return None;
    }
    if !scope_allows(&state.scope, &live.exe_path) {
        return None;
    }
    if !hosted_gate_allows(&live.exe_path, hwnd_u64, pid, &state.scope_hosts) {
        return None;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == hwnd_u64)?;
        if verify_proof_owned(hwnd_u64, entry, me).is_err() {
            return None;
        }
    }
    Some(crate::workspace::WindowKey {
        hwnd: hwnd_u64,
        pid,
        creation: live.process_creation,
    })
}

/// Track the born-fullscreen hold after one observation (KDE
/// initial-fullscreen-hold parity): lifetime-known non-fullscreen identities
/// accumulate here so a later fullscreen transition is a managed overlay,
/// never a born hold again; held keys observed non-fullscreen release for
/// normal fresh admission; hold and seen sets prune with the enumeration.
/// New holds are admitted in `ensure_workspace_assignments` (which owns
/// workspace membership, lifetime tags, and the held lifecycle log), never
/// here, so tracking never creates a non-member hold. Every observation path
/// funnels through `observe`, so one call here covers reconcile,
/// directional, workspace, gesture, and select ticks.
pub(super) fn track_fullscreen_holds(
    state: &mut TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    let mut live_nonfullscreen: BTreeSet<crate::workspace::WindowKey> = BTreeSet::new();
    for window in observed {
        live_nonfullscreen.insert(crate::workspace::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        });
    }
    for row in retained {
        if !row.fullscreen {
            live_nonfullscreen.insert(row.key.clone());
        }
    }
    // Lifetime-known non-fullscreen (KDE `seenNonFullscreen` parity): any
    // non-fullscreen observation, including minimized members, means a later
    // fullscreen transition is a managed overlay, never a born hold again.
    // Windows that could never verify holdable (scope, proof, safety gates)
    // suspend either way, so this never weakens a fence.
    state
        .seen_nonfullscreen
        .extend(live_nonfullscreen.iter().cloned());
    let log_path = state.log_path.clone();
    let released: Vec<crate::workspace::WindowKey> = state
        .born_fullscreen
        .iter()
        .filter(|key| live_nonfullscreen.contains(key) || !state.last_hwnds.contains(&key.hwnd))
        .cloned()
        .collect();
    for key in released {
        state.born_fullscreen.remove(&key);
        // R-SPC-04 D5: the first exit classifies with current hints and
        // predicate before maximize-clear/restore/slot side effects. The
        // entry persists until committed classification (see
        // `consume_classified_first_exits`): the next maximize-clear pass
        // only peeks at it, and assembly keeps classifying while it stands.
        state.born_first_exit.insert(key.clone());
        // Drop the hidden-snapshot seed so the first exit converges through
        // normal fresh admission (and the existing maximize clear when the
        // exit lands maximized) with no faked slot. The log below is the
        // once-per-lifecycle release line; held was logged at admission.
        if let Some(token) = state.member_tokens.get(&key).cloned() {
            state.member_rects.remove(&token);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        } else if let Some(token) = retained
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.token.clone())
        {
            state.member_rects.remove(&token);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        }
    }
    state
        .seen_nonfullscreen
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    state
        .born_first_exit
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    consume_classified_first_exits(state);
}

/// Consume first-exit marks whose classification committed (R-SPC-04 D5).
/// The retained Engine proves the outcome, so a classified mark must not
/// linger into later ticks: a lingering mark would force the tiled lane
/// again and could undo a later intentional float. Consumed when the live
/// client is Engine-automatic, a tiled member, or intentionally
/// floating/sticky. Anything else (no retained session yet, a refused
/// commit, or an uncommitted hold) keeps deferring: the clear keeps peeking
/// and assembly keeps classifying with no state reset. Runs at the end of
/// tracking, after releases insert fresh marks (a just-inserted mark still
/// shows its pre-commit Engine state and is never consumed here).
fn consume_classified_first_exits(state: &mut TileLoop) {
    let mut done = Vec::new();
    for key in state.born_first_exit.iter() {
        let Some(token) = state.member_tokens.get(key).cloned() else {
            continue;
        };
        // Intentional lanes own the window now (settled explicit float or
        // sticky, including restart-adopted intent): the exit already
        // resolved through them.
        if state.floated.contains(key) || state.sticky.contains_key(key) {
            done.push(key.clone());
            continue;
        }
        let Some(loc) = state.workspaces.member_loc(key) else {
            continue;
        };
        let domain_key = tiler_core::session::DomainKey {
            output: tiler_core::directional::OutputId(loc.output.clone()),
            workspace: tiler_core::directional::WorkspaceId(loc.workspace.clone()),
        };
        let Some(session) = state.engine.session(&domain_key) else {
            continue;
        };
        let id = WindowId(token.clone());
        if session.is_automatic_fixed_float(&id) {
            done.push(key.clone());
            continue;
        }
        if session
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window == id)
            && !session.is_exception(&id)
        {
            done.push(key.clone());
        }
        // An exception that is neither automatic nor intentional is
        // uncommitted (or a foreign hold): keep deferring.
    }
    for key in done {
        state.born_first_exit.remove(&key);
    }
}
