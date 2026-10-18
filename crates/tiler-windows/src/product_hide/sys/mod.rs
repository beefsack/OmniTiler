#![forbid(unsafe_code)]
use super::{
    HIDE_TIMEOUT_MS, REVEAL_TIMEOUT_MS, STORE_ACQUIRE_TIMEOUT_MS, WATCHER_POLL_MS,
    WATCHER_READY_TIMEOUT_MS, parse_watcher_ready, render_watcher_ready, watcher_ready_filename,
};
use crate::lifecycle::{exe_paths_equal, is_medium_rid};
use crate::model::{
    FLOAT_INTENT_PROP, MEMBER_TAG_PROP, PRODUCT_CLAIM_PROP, ProcessIdentity, ProductShowRestore,
    STICKY_PROP, TILE_OVERRIDE_PROP, WindowClaimKind, WindowIdentity, WindowShowState,
    generate_claim_tag, product_show_restore, valid_claim_tag, watcher_may_restore,
};
use crate::native::{HeldProcess, IdentityError};
use crate::storage::{LEDGER_FILE_NAME, LedgerStore};
use crate::workspace_owner::MarkerSlot;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod watcher;

pub use watcher::{
    cmd_watch_owner, reclaim_dead_residue, spawn_watcher, stop_watcher, verify_watcher_ready,
    watcher_ready_path,
};

type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

fn store_err(e: crate::storage::StorageError) -> DynError {
    match e {
        crate::storage::StorageError::Ledger(_) => err("refuse: corrupt ledger"),
        crate::storage::StorageError::LockContended => err("refuse: another owner holds the store"),
        crate::storage::StorageError::DifferentOwner => {
            err("refuse: ledger owner changed under lease")
        }
        crate::storage::StorageError::Io(io) => err(format!("error: ledger store: {io}")),
    }
}

/// Fresh read of the product nonce. `Ok(None)` means verifiably absent: a
/// recycled HWND starts without it.
fn get_nonce(hwnd_u64: u64) -> std::result::Result<Option<String>, String> {
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            return Ok(fake::with(hwnd_u64, |w| w.nonce.clone()).flatten());
        }
    }
    match crate::ffi::get_prop(hwnd_u64, PRODUCT_CLAIM_PROP) {
        None => Ok(None),
        Some(token) => {
            let token = token as u64;
            if token == 0 {
                return Ok(None);
            }
            Ok(Some(format!("{token:016x}")))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProductSnapshot {
    pub hwnd: u64,
    pub pid: u32,
    pub process: ProcessIdentity,
    pub nonce: Option<String>,
    pub visible: bool,
    pub iconic: bool,
    pub maximized: bool,
    pub class: String,
}

/// Admission read: HWND liveness, live PID, full process identity, nonce,
/// plus the hide exclusions (same session/SID medium, no shell/parented/
/// owned/tool/topmost/no-activate/dialog, no maximized or captionless
/// fullscreen). Only visible, unclaimed windows pass the callers below.
pub fn query_candidate(
    hwnd_u64: u64,
    owner: &ProcessIdentity,
) -> std::result::Result<ProductSnapshot, DynError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    };
    if hwnd_u64 == 0 {
        return Err(err("refuse: zero hwnd"));
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return Err(err("absent: hwnd destroyed"));
    }
    let pid = crate::ffi::window_pid(hwnd_u64);
    if pid == 0 {
        return Err(err("absent: hwnd no pid"));
    }
    if pid == owner.pid {
        return Err(err("refuse: self-owned"));
    }
    let held = match HeldProcess::open(pid) {
        Ok(h) => h,
        Err(IdentityError::Absent) => return Err(err("absent: target pid absent")),
        Err(e) => return Err(err(format!("error: target owner {e}"))),
    };
    let ident = match held.identity() {
        Ok(id) => id,
        Err(IdentityError::Absent) => return Err(err("absent: target pid absent")),
        Err(IdentityError::AccessDenied) => return Err(err("uncertain: target access-denied")),
        Err(e) => return Err(err(format!("error: target identity {e}"))),
    };
    if ident.pid != pid {
        return Err(err("absent: pid reuse"));
    }
    if ident.user_sid != owner.user_sid || ident.session_id != owner.session_id {
        return Err(err("refuse: sid/session mismatch"));
    }
    let rid = match held.integrity() {
        Ok(r) => r,
        Err(IdentityError::Absent) => return Err(err("absent: target pid absent")),
        Err(IdentityError::AccessDenied) => {
            return Err(err("uncertain: target integrity access-denied"));
        }
        Err(e) => return Err(err(format!("error: target integrity {e}"))),
    };
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    let class = crate::ffi::class_name(hwnd_u64);
    if super::shell_class_excluded(&class) {
        return Err(err("refuse: shell window"));
    }
    if crate::ffi::parent_of(hwnd_u64) != 0 {
        return Err(err("refuse: parented"));
    }
    if crate::ffi::owner_of(hwnd_u64) != 0 {
        return Err(err("refuse: owned dialog"));
    }
    let ex = crate::ffi::window_exstyle(hwnd_u64);
    if ex & WS_EX_TOOLWINDOW != 0 {
        return Err(err("refuse: tool window"));
    }
    if ex & WS_EX_TOPMOST != 0 {
        return Err(err("refuse: topmost"));
    }
    if ex & WS_EX_NOACTIVATE != 0 {
        return Err(err("refuse: no-activate"));
    }
    // Own Settings is an ordinary slotless member (never a dialog);
    // generic unowned `#32770` dialogs still refuse.
    if crate::tiling::workspace_dialog_refuses(&class, false, &ident.exe_path, &owner.exe_path) {
        return Err(err("refuse: dialog"));
    }
    {
        if crate::ffi::is_zoomed(hwnd_u64) {
            return Err(err("refuse: fullscreen"));
        }
        let style = crate::ffi::window_style(hwnd_u64);
        if style & WS_CAPTION == 0 {
            return Err(err("refuse: borderless (possible fullscreen)"));
        }
    }
    let nonce = get_nonce(hwnd_u64).map_err(|e| err(format!("error: nonce read {e}")))?;
    let visible = crate::ffi::is_visible(hwnd_u64);
    let iconic = crate::ffi::is_iconic(hwnd_u64);
    Ok(ProductSnapshot {
        hwnd: hwnd_u64,
        pid,
        process: ident,
        nonce,
        visible,
        iconic,
        maximized: crate::ffi::is_zoomed(hwnd_u64),
        class,
    })
}

/// Recovery read: HWND liveness, live PID, full process identity, nonce,
/// visibility. No admission exclusions: a safely hidden window may change
/// maximized, captionless, topmost, or owned state while hidden, and
/// recovery reveals exact process plus nonce regardless. Unknown identity
/// errors are uncertain, never absent.
pub fn query_recovery(
    hwnd_u64: u64,
    owner: &ProcessIdentity,
) -> std::result::Result<ProductSnapshot, DynError> {
    #[cfg(test)]
    {
        let _ = owner;
        if fake::active(hwnd_u64) {
            return fake::snapshot(hwnd_u64).ok_or_else(|| err("absent: hwnd destroyed"));
        }
    }
    if hwnd_u64 == 0 {
        return Err(err("refuse: zero hwnd"));
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return Err(err("absent: hwnd destroyed"));
    }
    let pid = crate::ffi::window_pid(hwnd_u64);
    if pid == 0 {
        return Err(err("absent: hwnd no pid"));
    }
    let held = match HeldProcess::open(pid) {
        Ok(h) => h,
        Err(IdentityError::Absent) => return Err(err("absent: target pid absent")),
        Err(IdentityError::AccessDenied) => {
            return Err(err("uncertain: target access-denied"));
        }
        Err(e) => return Err(err(format!("error: target owner {e}"))),
    };
    let ident = match held.identity() {
        Ok(id) => id,
        Err(IdentityError::Absent) => return Err(err("absent: target pid absent")),
        Err(IdentityError::AccessDenied) => return Err(err("uncertain: target access-denied")),
        Err(e) => return Err(err(format!("error: target identity {e}"))),
    };
    if ident.pid != pid {
        return Err(err("absent: pid reuse"));
    }
    if ident.user_sid != owner.user_sid || ident.session_id != owner.session_id {
        return Err(err("refuse: sid/session mismatch"));
    }
    let nonce = get_nonce(hwnd_u64).map_err(|e| err(format!("error: nonce read {e}")))?;
    let visible = crate::ffi::is_visible(hwnd_u64);
    let iconic = crate::ffi::is_iconic(hwnd_u64);
    Ok(ProductSnapshot {
        hwnd: hwnd_u64,
        pid,
        process: ident,
        nonce,
        visible,
        iconic,
        maximized: crate::ffi::is_zoomed(hwnd_u64),
        class: String::new(),
    })
}

/// Install a fresh random nonzero window-lifetime nonce, then read it
/// back. Only call after [`query_candidate`] admitted the window; the
/// caller commits the returned tag BEFORE hiding.
pub fn install_nonce(hwnd_u64: u64, owner_pid: u32) -> Result<String> {
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            if fake::with(hwnd_u64, |w| w.alive).unwrap_or(false) {
                let tag = generate_claim_tag(owner_pid);
                if valid_claim_tag(&tag) {
                    fake::with_mut(hwnd_u64, |w| w.nonce = Some(tag.clone()));
                    return Ok(tag);
                }
                return Err(err("error: nonce install failed"));
            }
            return Err(err("error: SetProp failed"));
        }
    }
    for _ in 0..4 {
        let tag = generate_claim_tag(owner_pid);
        debug_assert!(valid_claim_tag(&tag));
        let token = u64::from_str_radix(&tag, 16).map_err(|_| err("error: nonce render"))?;
        if token == 0 {
            continue;
        }
        if !crate::ffi::set_prop(hwnd_u64, PRODUCT_CLAIM_PROP, token as usize) {
            return Err(err("error: SetProp failed"));
        }
        match get_nonce(hwnd_u64).map_err(|e| err(format!("error: nonce readback {e}")))? {
            Some(back) if back == tag => return Ok(tag),
            _ => {
                let _ = remove_nonce_inner(hwnd_u64);
                return Err(err("error: nonce readback mismatch"));
            }
        }
    }
    Err(err("error: nonce install failed"))
}

fn remove_nonce_inner(hwnd_u64: u64) -> bool {
    let removed = crate::ffi::take_prop(hwnd_u64, PRODUCT_CLAIM_PROP).is_some();
    removed || get_nonce(hwnd_u64).ok().flatten().is_none()
}

/// Fresh read of the visible-membership lifetime tag. `None` means
/// verifiably no tag: a same-process HWND reuse starts without our
/// window-lifetime property, and a destroyed HWND reads back nothing.
/// Read-only; never mutates the window.
pub fn read_member_tag(hwnd_u64: u64) -> Option<String> {
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            return fake::with(hwnd_u64, |w| w.member_tag.clone()).flatten();
        }
    }
    let token = crate::ffi::get_prop(hwnd_u64, MEMBER_TAG_PROP)? as u64;
    if token == 0 {
        return None;
    }
    let tag = format!("{token:016x}");
    if valid_claim_tag(&tag) {
        Some(tag)
    } else {
        None
    }
}

/// Stamp a fresh random window-lifetime tag for visible membership, then
/// read it back. Only call when admitting a window with no trusted stored
/// tag (first sight or same-process reuse repair), after the caller bound
/// the window to its expected identity. Overwrites only our own property
/// name, which no other writer uses. A readback mismatch fails closed with
/// no membership and no removal: the window died mid-install (the new
/// generation owns the number without our property) or a foreign value
/// appeared, and removing here could strip another owner's tag. Inert
/// residue fails closed on its own.
pub fn install_member_tag(hwnd_u64: u64, owner_pid: u32) -> Result<String> {
    for _ in 0..4 {
        let tag = generate_claim_tag(owner_pid);
        debug_assert!(valid_claim_tag(&tag));
        let Ok(token) = u64::from_str_radix(&tag, 16) else {
            return Err(err("error: member tag render"));
        };
        if token == 0 {
            continue;
        }
        if !crate::ffi::set_prop(hwnd_u64, MEMBER_TAG_PROP, token as usize) {
            return Err(err("error: SetProp failed"));
        }
        match read_member_tag(hwnd_u64) {
            Some(back) if back == tag => return Ok(tag),
            _ => return Err(err("error: member tag readback mismatch")),
        }
    }
    Err(err("error: member tag install failed"))
}

/// Tag-only nonce equality: the live window-lifetime property equals the
/// expected claim tag. Identity fencing stays with the caller.
#[must_use]
pub fn nonce_tag_matches(live: Option<&str>, expected: &str) -> bool {
    live.is_some_and(|tag| tag == expected)
}

/// One honest `GetPropW` read of our own property name. The `GetPropW`
/// contract promises NULL when the list does not contain the string and
/// promises no last-error distinction, so NULL reads as
/// [`MarkerSlot::Absent`] unconditionally: absent reads on real windows
/// report a nonzero error (e.g. `ERROR_FILE_NOT_FOUND`), which must
/// never diagnose as [`MarkerSlot::Unreadable`]. A destroyed HWND reads
/// absent (the marker dies with the window). Same convention as
/// [`get_nonce`], [`read_member_tag`], and the helper lifetime read.
fn read_prop_slot(hwnd_u64: u64, name: &str) -> MarkerSlot {
    if !crate::ffi::is_window(hwnd_u64) {
        return MarkerSlot::Absent;
    }
    match crate::ffi::get_prop(hwnd_u64, name) {
        None => MarkerSlot::Absent,
        // Non-NULL handles are nonzero; zero is unrepresentable here
        // (NULL reads absent above), so `Value(0)` never escapes.
        Some(v) => MarkerSlot::Value(v as u64),
    }
}

/// Sticky-float marker slot: absent is silent no-intent, a wrong value
/// is logged no-intent, a failed read is diagnosed no-intent.
/// Read-only. A recycled HWND starts without our property.
pub fn peek_sticky_raw(hwnd_u64: u64) -> MarkerSlot {
    read_prop_slot(hwnd_u64, STICKY_PROP)
}

/// Intentional-float marker slot. Same honest absences as
/// [`peek_sticky_raw`]. Registered test fakes read their scripted slot.
pub fn peek_float_intent_raw(hwnd_u64: u64) -> MarkerSlot {
    #[cfg(test)]
    if let Some(slot) = fake::peek_float_mark(hwnd_u64) {
        return match slot {
            Some(value) => MarkerSlot::Value(value),
            None => MarkerSlot::Absent,
        };
    }
    read_prop_slot(hwnd_u64, FLOAT_INTENT_PROP)
}

/// Reserved tile-override marker slot (item 13 D7). No admission path
/// reads it in item 8.
#[allow(dead_code)]
pub fn peek_tile_override_raw(hwnd_u64: u64) -> MarkerSlot {
    read_prop_slot(hwnd_u64, TILE_OVERRIDE_PROP)
}

/// Fresh read of the sticky-float marker. `None` means not sticky:
/// absent, unreadable, zero, or unknown values fail closed. Read-only.
pub fn read_sticky_marker(hwnd_u64: u64) -> Option<bool> {
    match peek_sticky_raw(hwnd_u64) {
        MarkerSlot::Value(value) => crate::tiling::parse_sticky_marker(value),
        MarkerSlot::Absent | MarkerSlot::Unreadable => None,
    }
}

/// Stamp the sticky-float marker for one verified managed window, then
/// read it back. Only call after the caller bound the window to its
/// expected full identity plus lifetime tag, scope, hosted-child, and
/// proof fences. Overwrites only our own property name. A readback
/// mismatch fails closed with no runtime map change and no removal.
pub fn install_sticky_marker(hwnd_u64: u64, prior_floating: bool) -> Result<()> {
    let value = crate::tiling::sticky_marker_value(prior_floating);
    if !crate::ffi::set_prop(hwnd_u64, STICKY_PROP, value as usize) {
        return Err(err("error: SetProp failed"));
    }
    match read_sticky_marker(hwnd_u64) {
        Some(back) if back == prior_floating => Ok(()),
        _ => Err(err("error: sticky marker readback mismatch")),
    }
}

/// Remove the sticky marker only when the live value still equals the
/// expected pre-sticky state. Returns `true` when verifiably absent
/// afterwards (removed or already absent), `false` when a different value
/// owns the window now (never touch it). A failed read errors without
/// mutating. Identity fencing stays with the caller.
pub fn remove_sticky_marker(hwnd_u64: u64, expected_prior: bool) -> Result<bool> {
    if !crate::ffi::is_window(hwnd_u64) {
        return Ok(true);
    }
    match peek_sticky_raw(hwnd_u64) {
        MarkerSlot::Absent => return Ok(true),
        MarkerSlot::Unreadable => return Err(err("error: sticky marker read error")),
        MarkerSlot::Value(value) => {
            if crate::tiling::parse_sticky_marker(value) != Some(expected_prior) {
                return Ok(false);
            }
        }
    }
    crate::ffi::remove_prop(hwnd_u64, STICKY_PROP);
    match peek_sticky_raw(hwnd_u64) {
        MarkerSlot::Absent => Ok(true),
        MarkerSlot::Unreadable => Err(err("error: sticky marker read error")),
        MarkerSlot::Value(_) => Err(err("error: sticky marker remove failed")),
    }
}

/// Fresh read of the intentional-float marker. `true` only when the
/// value parses (1); everything else fails closed. Read-only.
pub fn read_float_intent_marker(hwnd_u64: u64) -> bool {
    matches!(
        peek_float_intent_raw(hwnd_u64),
        MarkerSlot::Value(value)
            if crate::tiling::parse_float_intent_marker(value).is_some()
    )
}

/// Fresh read of the reserved tile-override marker. No admission path
/// reads it in item 8.
#[allow(dead_code)]
pub fn read_tile_override_marker(hwnd_u64: u64) -> bool {
    matches!(
        peek_tile_override_raw(hwnd_u64),
        MarkerSlot::Value(value)
            if crate::tiling::parse_tile_override_marker(value).is_some()
    )
}

/// Stamp the intentional-float marker for one verified managed window,
/// then read it back. Only call after the caller bound the window to its
/// expected full identity plus lifetime tag, scope, hosted-child, and
/// proof fences, and only after the native float circa Engine commit
/// verified. A readback mismatch fails closed with no runtime map change
/// and no removal.
pub fn install_float_intent_marker(hwnd_u64: u64) -> Result<()> {
    let value = crate::tiling::float_intent_marker_value();
    #[cfg(test)]
    if fake::active(hwnd_u64) {
        if !fake::store_float_mark(hwnd_u64, value) {
            return Err(err("error: SetProp failed"));
        }
        return match peek_float_intent_raw(hwnd_u64) {
            MarkerSlot::Value(back) if crate::tiling::parse_float_intent_marker(back).is_some() => {
                Ok(())
            }
            MarkerSlot::Unreadable => Err(err("error: float intent marker read error")),
            _ => Err(err("error: float intent marker readback mismatch")),
        };
    }
    if !crate::ffi::set_prop(hwnd_u64, FLOAT_INTENT_PROP, value as usize) {
        return Err(err("error: SetProp failed"));
    }
    match peek_float_intent_raw(hwnd_u64) {
        MarkerSlot::Value(back) if crate::tiling::parse_float_intent_marker(back).is_some() => {
            Ok(())
        }
        MarkerSlot::Unreadable => Err(err("error: float intent marker read error")),
        _ => Err(err("error: float intent marker readback mismatch")),
    }
}

/// Stamp the reserved tile-override marker, then read it back. Reserved
/// for item 13 D7; no caller in item 8.
#[allow(dead_code)]
pub fn install_tile_override_marker(hwnd_u64: u64) -> Result<()> {
    let value = crate::tiling::tile_override_marker_value();
    if !crate::ffi::set_prop(hwnd_u64, TILE_OVERRIDE_PROP, value as usize) {
        return Err(err("error: SetProp failed"));
    }
    match peek_tile_override_raw(hwnd_u64) {
        MarkerSlot::Value(back) if crate::tiling::parse_tile_override_marker(back).is_some() => {
            Ok(())
        }
        MarkerSlot::Unreadable => Err(err("error: tile override marker read error")),
        _ => Err(err("error: tile override marker readback mismatch")),
    }
}

/// Remove the intentional-float marker on settled unfloat. Returns
/// `Ok(())` when verifiably absent afterwards (removed, already absent,
/// or a destroyed HWND); errors on a failed read or remove without
/// mutating further. Corrupt residue on our own property name is
/// cleaned under the caller's identity plus lifetime fences.
pub fn remove_float_intent_marker(hwnd_u64: u64) -> Result<()> {
    if !crate::ffi::is_window(hwnd_u64) {
        return Ok(());
    }
    match peek_float_intent_raw(hwnd_u64) {
        MarkerSlot::Absent => return Ok(()),
        MarkerSlot::Unreadable => {
            return Err(err("error: float intent marker read error"));
        }
        MarkerSlot::Value(_) => {}
    }
    crate::ffi::remove_prop(hwnd_u64, FLOAT_INTENT_PROP);
    match peek_float_intent_raw(hwnd_u64) {
        MarkerSlot::Absent => Ok(()),
        MarkerSlot::Unreadable => Err(err("error: float intent marker read error")),
        MarkerSlot::Value(_) => Err(err("error: float intent marker remove failed")),
    }
}

/// Remove the reserved tile-override marker. Reserved for item 13 D7;
/// no caller in item 8.
#[allow(dead_code)]
pub fn remove_tile_override_marker(hwnd_u64: u64) -> Result<()> {
    if !crate::ffi::is_window(hwnd_u64) {
        return Ok(());
    }
    match peek_tile_override_raw(hwnd_u64) {
        MarkerSlot::Absent => return Ok(()),
        MarkerSlot::Unreadable => {
            return Err(err("error: tile override marker read error"));
        }
        MarkerSlot::Value(_) => {}
    }
    crate::ffi::remove_prop(hwnd_u64, TILE_OVERRIDE_PROP);
    match peek_tile_override_raw(hwnd_u64) {
        MarkerSlot::Absent => Ok(()),
        MarkerSlot::Unreadable => Err(err("error: tile override marker read error")),
        MarkerSlot::Value(_) => Err(err("error: tile override marker remove failed")),
    }
}

/// Remove the nonce only when the live window still carries the expected
/// full process identity plus the exact claim tag. Verifies through the
/// identity-only recovery read before `RemoveProp`, holds the target
/// alive across the removal, and rechecks liveness plus the pid after.
/// Returns `true` when verifiably absent afterwards, `false` when a
/// different identity or tag owns the window now (recycled: retire the
/// claim, never touch the new owner). Never writes foreign windows.
pub fn remove_nonce_checked(
    hwnd_u64: u64,
    expected_tag: &str,
    expected_process: &ProcessIdentity,
    owner: &ProcessIdentity,
) -> Result<bool> {
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            // Test-only mirror of the production fencing below against the
            // fake: destroyed reads absent (nothing to remove), process/tag
            // mismatch retires without touching, exact match removes.
            let snap = match fake::snapshot(hwnd_u64) {
                Some(s) => s,
                None => return Ok(true),
            };
            if snap.process != *expected_process {
                return Ok(false);
            }
            match snap.nonce.as_deref() {
                None => return Ok(true),
                Some(tag) if nonce_tag_matches(Some(tag), expected_tag) => {}
                Some(_) => return Ok(false),
            }
            fake::with_mut(hwnd_u64, |w| w.nonce = None);
            return Ok(true);
        }
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return Ok(true);
    }
    let snap = match query_recovery(hwnd_u64, owner) {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("absent:") || msg.contains("sid/session mismatch") {
                return Ok(true);
            }
            return Err(err(format!("uncertain: nonce remove pre-read {msg}")));
        }
    };
    if snap.process != *expected_process {
        return Ok(false);
    }
    match snap.nonce.as_deref() {
        None => return Ok(true),
        Some(tag) if nonce_tag_matches(Some(tag), expected_tag) => {}
        Some(_) => return Ok(false),
    }
    let held = match hold_target_verified(snap.pid, &snap.process) {
        Ok(h) => h,
        Err(e) if e.to_string().starts_with("absent:") => return Ok(false),
        Err(e) => return Err(err(format!("uncertain: nonce remove held {e}"))),
    };
    let _ = remove_nonce_inner(hwnd_u64);
    let _ = &held;
    if !held.is_alive() || !pid_current(hwnd_u64, snap.pid) {
        return Err(err("uncertain: nonce remove post-pid changed"));
    }
    match get_nonce(hwnd_u64).map_err(|e| err(format!("error: nonce remove readback {e}")))? {
        None => Ok(true),
        Some(_) => Err(err("error: nonce remove failed")),
    }
}

fn pump_once() {
    crate::ffi::drain_messages();
}

/// Post one visibility effect without blocking, then pump until the
/// readback observes it. `ShowWindowAsync` queues behind earlier posts so
/// hide/show ordering per window is preserved, a hung target cannot wedge
/// the caller, and no orphan setter thread outlives the call. Success
/// requires the observed effect, never the post alone.
fn post_visibility(hwnd_u64: u64, cmd: i32, expect_visible: bool, timeout_ms: u64) -> Result<()> {
    if !post_async(hwnd_u64, cmd) {
        return Err(err("error: visibility post failed"));
    }
    let start = Instant::now();
    let budget = Duration::from_millis(timeout_ms);
    loop {
        pump_once();
        // Fake-aware readback (identical native calls in production), so the
        // shared post semantics hold on both paths.
        match batch_is_visible(hwnd_u64) {
            None => return Err(err("absent: hwnd destroyed")),
            Some(visible) if visible == expect_visible => return Ok(()),
            Some(_) => {}
        }
        if start.elapsed() >= budget {
            return Err(err("uncertain: visibility not observed"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// ---- Joint batch primitives (select/send/rollback hide+reveal) ---- ///
/// Shared prepare/post/finish behind both the single-window
/// [`hide_managed_claim`] / [`reveal_product_claim_to`] below and the owner
/// joint batch driver (`tiling_sys::visibility_batch`), so batch and
/// remaining single-window paths cannot diverge in safety: identical
/// commit-before-hide, identical identity/nonce/ledger ownership, identical
/// failure recovery. Only the wait differs: single paths keep the legacy
/// per-window pumped bound; batch phases post every eligible effect first
/// and confirm them together under one shared deadline (see
/// [`super::batch`]).
///
/// A prepared hide owns its target across the batch through the returned
/// [`HeldProcess`] guard: the caller keeps every guard alive until its
/// finish ran, exactly like the single path keeps `held` across its post.
///
/// Shape refusal for one product claim: kind plus tag format. Shape
/// refusals never trigger orphan cleanup and never touch the ledger.
pub(crate) fn check_claim_shape(claim: &WindowIdentity) -> Result<()> {
    use crate::model::WindowClaimKind;
    if claim.kind != WindowClaimKind::Product {
        return Err(err("refuse: not a product claim"));
    }
    if !valid_claim_tag(&claim.tag) {
        return Err(err("refuse: bad product tag"));
    }
    Ok(())
}

/// Watcher, identity-only pre-read, target hold, and durable preimage for
/// one managed hide, without any ledger or visibility effect. The caller
/// validates shape first ([`check_claim_shape`]: shape refusals take no
/// cleanup) and commits ([`commit_hide_claim`]) before posting.
pub(crate) fn pre_hide_checks(
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
) -> Result<(ProductSnapshot, HeldProcess)> {
    check_claim_shape(claim)?;
    verify_watcher_ready(dir, owner)?;
    let pre = query_recovery(claim.hwnd, owner)?;
    if pre.pid != claim.process.pid
        || pre.process != claim.process
        || pre.nonce.as_deref() != Some(claim.tag.as_str())
        || !pre.visible
    {
        return Err(err("refuse: hide pre-readback mismatch"));
    }
    let held = hold_target_verified(pre.pid, &pre.process)?;
    Ok((pre, held))
}

/// Durable ledger preimage for one managed hide: the fresh iconic/maximized
/// show state captured from the pre-hide read, so independent restore
/// returns the pre-hide state even on paths that never saw the hide.
#[must_use]
pub(crate) fn durable_hide_claim(claim: &WindowIdentity, pre: &ProductSnapshot) -> WindowIdentity {
    WindowIdentity {
        show: WindowShowState {
            minimized: pre.iconic,
            maximized: pre.maximized,
        },
        ..claim.clone()
    }
}

/// Commit-before-hide: append the durable claim to the recovery ledger.
/// No visibility effect. The claim is owned from here even when the later
/// post/readback reports uncertain. Times only the store commit into
/// `ledger_ms`.
pub(crate) fn commit_hide_claim(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    durable: &WindowIdentity,
    ledger_ms: &mut u64,
) -> Result<()> {
    let t0 = std::time::Instant::now();
    let result = append_product_claim(store, owner, durable);
    *ledger_ms =
        ledger_ms.saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    result
}

/// Best-effort orphan cleanup for an admitted-but-uncommitted nonce (shared
/// with [`hide_managed_claim`]): removes the window-lifetime nonce ONLY when
/// the fresh committed ledger proves no same-HWND product claim exists AND
/// the live window still carries the full expected process plus the exact
/// nonce. Never fails: the caller's error stands.
pub(crate) fn cleanup_orphan_nonce(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
) {
    maybe_cleanup_orphan(store, owner, claim);
}

/// Post one async visibility effect without blocking or waiting:
/// `ShowWindowAsync` queues behind earlier posts so per-window ordering is
/// preserved and a hung target cannot wedge the caller. Shared by the
/// legacy per-window poll and the joint batch posts, so both report the
/// identical post verdict. Returns `false` when the post itself refused.
fn post_async(hwnd_u64: u64, cmd: i32) -> bool {
    #[cfg(test)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
        if fake::active(hwnd_u64) {
            let hide = cmd == SW_HIDE;
            fake::record_post(hwnd_u64, hide);
            return fake::with_mut(hwnd_u64, |w| {
                if !w.alive || !w.post_ok {
                    return false;
                }
                if !w.defer {
                    w.visible = !hide;
                }
                true
            })
            .unwrap_or(false);
        }
    }
    crate::ffi::show_window_async(hwnd_u64, cmd)
}

/// Fake-aware HWND liveness for the batch prepare path: the scripted `alive`
/// flag offline, `IsWindow` in production. Production never inspects
/// fabricated HWNDs.
fn live_hwnd(hwnd_u64: u64) -> bool {
    #[cfg(test)]
    if fake::active(hwnd_u64) {
        return fake::with(hwnd_u64, |w| w.alive).unwrap_or(false);
    }
    crate::ffi::is_window(hwnd_u64)
}
/// Post one hide effect without blocking or waiting (same async contract
/// as the legacy poll's post, via the shared [`post_async`]); confirmation
/// still requires an observed snapshot, never the post alone.
pub(crate) fn batch_post_hide(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
    post_async(hwnd_u64, SW_HIDE)
}

/// Nonactivating show command for one reveal, from the durable ledger
/// preimage: originally minimized restores minimized (`SW_SHOWMINNOACTIVE`),
/// otherwise `SW_SHOWNA`, which preserves native maximized placement. The
/// `minimized` parameter carries the in-memory hide-time state for
/// preimage-less (v3) claims; a durable minimized preimage always wins.
#[must_use]
pub(crate) fn reveal_show_cmd(claim: &WindowIdentity, minimized: bool) -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWMINNOACTIVE, SW_SHOWNA};
    match product_show_restore(WindowShowState {
        minimized: claim.show.minimized || minimized,
        maximized: claim.show.maximized,
    }) {
        ProductShowRestore::ShowMinimized => SW_SHOWMINNOACTIVE,
        ProductShowRestore::ShowNormal | ProductShowRestore::ShowMaximized => SW_SHOWNA,
    }
}

/// Post one reveal effect without blocking or waiting (same async contract
/// as [`batch_post_hide`]). The caller computed `show_cmd` via
/// [`reveal_show_cmd`].
pub(crate) fn batch_post_reveal(hwnd_u64: u64, show_cmd: i32) -> bool {
    post_async(hwnd_u64, show_cmd)
}

/// Fresh visibility readback for one batch member: `Some(visible)` on a
/// live window, `None` when verifiably destroyed (`IsWindow` false).
/// Production never inspects fabricated HWNDs.
pub(crate) fn batch_is_visible(hwnd_u64: u64) -> Option<bool> {
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            fake::record_snapshot(hwnd_u64);
            return fake::with(hwnd_u64, |w| w.alive.then_some(w.visible)).flatten();
        }
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return None;
    }
    Some(crate::ffi::is_visible(hwnd_u64))
}

/// Post-hide finish for one batch member: the post-pid fence plus the
/// identity/nonce/hidden readback. The ledger entry (committed before the
/// post) is kept on mismatch, exactly like the single path.
pub(crate) fn finish_hide_confirm(
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    held: &HeldProcess,
    pre_pid: u32,
) -> Result<()> {
    if !held.is_alive() || !pid_current(claim.hwnd, pre_pid) {
        return Err(err("uncertain: hide post-pid changed"));
    }
    let back = query_recovery(claim.hwnd, owner)?;
    if back.pid != claim.process.pid
        || back.process != claim.process
        || back.nonce.as_deref() != Some(claim.tag.as_str())
        || back.visible
    {
        return Err(err("uncertain: hide readback mismatch (kept ledger)"));
    }
    Ok(())
}

/// One prepared reveal awaiting its post: the live pid plus hold plus show
/// command. Already-visible and retired claims never reach this: they
/// resolve during preparation with no post.
pub(crate) struct PreparedReveal {
    pub(crate) held: HeldProcess,
    pub(crate) pid: u32,
    pub(crate) show_cmd: i32,
}

/// Reveal readiness: either a verified post to issue or a settled teardown
/// that needed no post.
pub(crate) enum RevealReady {
    Post(PreparedReveal),
    Done(super::ProductTeardown),
}

/// Reveal preparation through the posted phase: liveness, identity-only
/// pre-read with recycle fencing, already-visible short-circuit (nonce plus
/// note removal, exactly like the single path), target hold, and the
/// nonactivating show command from the durable preimage (maximized/
/// minimized restoration preserved). `Err` carries the single-path
/// vocabulary: `absent:*` (caller retires) versus `uncertain:*` (caller
/// keeps the ledger). No visibility effect.
pub(crate) fn prepare_reveal(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
    ledger_ms: &mut u64,
) -> Result<RevealReady> {
    use super::ProductTeardown;
    use crate::model::WindowClaimKind;
    if claim.kind != WindowClaimKind::Product {
        return Err(err("refuse: not a product claim"));
    }
    let hwnd = claim.hwnd;
    if !live_hwnd(hwnd) {
        remove_product_note(store, owner, hwnd, ledger_ms)?;
        return Ok(RevealReady::Done(ProductTeardown::Retired));
    }
    let snap = match query_recovery(hwnd, owner) {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("absent:") {
                remove_product_note(store, owner, hwnd, ledger_ms)?;
                return Ok(RevealReady::Done(ProductTeardown::Retired));
            }
            if msg.contains("sid/session mismatch") {
                remove_product_note(store, owner, hwnd, ledger_ms)?;
                return Ok(RevealReady::Done(ProductTeardown::Retired));
            }
            return Err(err(format!("uncertain: reveal pre-read {msg}")));
        }
    };
    if snap.process != claim.process || snap.nonce.as_deref() != Some(claim.tag.as_str()) {
        remove_product_note(store, owner, hwnd, ledger_ms)?;
        return Ok(RevealReady::Done(ProductTeardown::Retired));
    }
    if snap.visible {
        remove_nonce_checked(hwnd, &claim.tag, &claim.process, owner)?;
        remove_product_note(store, owner, hwnd, ledger_ms)?;
        return Ok(RevealReady::Done(ProductTeardown::AlreadyVisible));
    }
    let held = match hold_target_verified(snap.pid, &snap.process) {
        Ok(h) => h,
        Err(e) if e.to_string().starts_with("absent:") => {
            remove_product_note(store, owner, hwnd, ledger_ms)?;
            return Ok(RevealReady::Done(ProductTeardown::Retired));
        }
        Err(e) => return Err(err(format!("uncertain: reveal held {e}"))),
    };
    let show_cmd = reveal_show_cmd(claim, minimized);
    Ok(RevealReady::Post(PreparedReveal {
        held,
        pid: snap.pid,
        show_cmd,
    }))
}

/// Post-reveal finish with the pre-post hold: when `guard` carries the
/// prepared hold plus pid, the post-pid fence runs exactly like the single
/// path; `None` skips only the fence (used where the hold could not be
/// carried, never in batch phases).
pub(crate) fn finish_reveal_guarded(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    guard: Option<(&HeldProcess, u32)>,
    ledger_ms: &mut u64,
) -> Result<super::ProductTeardown> {
    use super::ProductTeardown;
    let hwnd = claim.hwnd;
    if let Some((held, pid)) = guard {
        let _ = held;
        if !held.is_alive() || !pid_current(hwnd, pid) {
            return Err(err("uncertain: reveal post-pid changed"));
        }
    }
    let back =
        query_recovery(hwnd, owner).map_err(|e| err(format!("uncertain: reveal readback {e}")))?;
    if back.process != claim.process
        || back.nonce.as_deref() != Some(claim.tag.as_str())
        || !back.visible
    {
        return Err(err("uncertain: reveal readback mismatch (kept ledger)"));
    }
    remove_nonce_checked(hwnd, &claim.tag, &claim.process, owner)?;
    remove_product_note(store, owner, hwnd, ledger_ms)?;
    Ok(ProductTeardown::Revealed)
}

pub fn hold_target_verified(pid: u32, expected: &ProcessIdentity) -> Result<HeldProcess> {
    #[cfg(test)]
    {
        if let Some(verdict) = fake::hold_verified(pid, expected) {
            return verdict;
        }
    }
    let held = match HeldProcess::open(pid) {
        Ok(h) => h,
        Err(IdentityError::Absent) => return Err(err("absent: target pid absent")),
        Err(e) => return Err(err(format!("error: target held {e}"))),
    };
    let live = match held.identity() {
        Ok(l) => l,
        Err(IdentityError::Absent) => return Err(err("absent: target pid absent")),
        Err(e) => return Err(err(format!("error: target identity {e}"))),
    };
    if live != *expected || !held.is_alive() {
        return Err(err("absent: target identity changed (recycled)"));
    }
    Ok(held)
}

fn pid_current(hwnd_u64: u64, pid: u32) -> bool {
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            return fake::with(hwnd_u64, |w| w.alive && w.pid == pid).unwrap_or(false);
        }
    }
    let current = crate::ffi::window_pid(hwnd_u64);
    current != 0 && current == pid
}

/// Typed admission read for one hide candidate. Every gate is a typed
/// variant: no error-string matching selects security outcomes.
/// `Admissible` passes the full strict gates (visible, unclaimed, not
/// zoomed/captionless/iconic); `Retained` passes every ownership gate but
/// is minimized, maximized, or captionless-fullscreen, so only an exact
/// already-eligible member may hide it. Recovery stays identity-only.
#[derive(Debug)]
pub enum CandidateStatus {
    Admissible(ProductSnapshot),
    Retained(ProductSnapshot),
    Absent,
    Uncertain,
    Refused(&'static str),
}

/// Typed failure of managed admission. The caller maps variants to its
/// closed outcome vocabulary; strings never select the outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedAdmitError {
    Absent,
    Uncertain,
    WrongIdentity,
    Refused(&'static str),
}

/// Classify one hide candidate with typed gates: live HWND, live PID,
/// full process identity, same SID/session, medium integrity,
/// shell/parented/owned/tool/topmost/no-activate/dialog exclusions,
/// unclaimed nonce, visibility. Geometry gates (`IsZoomed`, captionless,
/// iconic) select `Retained` instead of refusal so retained members stay
/// hideable.
pub fn classify_candidate(hwnd_u64: u64, owner: &ProcessIdentity) -> CandidateStatus {
    classify_candidate_for_hide(hwnd_u64, owner, false)
}

/// Hide-path classification with a narrow project-raised topmost
/// exception: when `allow_project_topmost` is true the live `TOPMOST`
/// band no longer refuses and the remaining identity plus live style
/// gates still apply. The caller authenticates the band via
/// [`super::project_topmost_hide_allowed`] (exact runtime key
/// `Some(false)` plus live band) combined with the fresh full admitted
/// identity and window-lifetime gates in the managed admission below; a
/// marker alone never sets this bit. All other admission paths keep the
/// blanket refusal via [`classify_candidate`].
pub fn classify_candidate_for_hide(
    hwnd_u64: u64,
    owner: &ProcessIdentity,
    allow_project_topmost: bool,
) -> CandidateStatus {
    #[cfg(test)]
    {
        let _ = allow_project_topmost;
        if fake::active(hwnd_u64) {
            // Test-only mirror of the production gates above against the
            // fake: liveness, self-ownership, claim state, and visibility
            // select Admissible/Retained; geometry gates collapse to
            // maximized/iconic (the fake models no styles or owners).
            return fake::with(hwnd_u64, |w| {
                if !w.alive {
                    return CandidateStatus::Absent;
                }
                if w.pid == owner.pid {
                    return CandidateStatus::Refused("self-owned");
                }
                if w.nonce.is_some() {
                    return CandidateStatus::Refused("already-claimed");
                }
                if !w.visible {
                    return CandidateStatus::Refused("not-visible");
                }
                let snap = ProductSnapshot {
                    hwnd: hwnd_u64,
                    pid: w.pid,
                    process: w.process.clone(),
                    nonce: w.nonce.clone(),
                    visible: w.visible,
                    iconic: w.iconic,
                    maximized: w.maximized,
                    class: String::new(),
                };
                if w.maximized || w.iconic {
                    CandidateStatus::Retained(snap)
                } else {
                    CandidateStatus::Admissible(snap)
                }
            })
            .unwrap_or(CandidateStatus::Absent);
        }
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    };
    if hwnd_u64 == 0 {
        return CandidateStatus::Absent;
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return CandidateStatus::Absent;
    }
    let pid = crate::ffi::window_pid(hwnd_u64);
    if pid == 0 {
        return CandidateStatus::Absent;
    }
    if pid == owner.pid {
        return CandidateStatus::Refused("self-owned");
    }
    let held = match HeldProcess::open(pid) {
        Ok(h) => h,
        Err(IdentityError::Absent) => return CandidateStatus::Absent,
        Err(_) => return CandidateStatus::Uncertain,
    };
    let ident = match held.identity() {
        Ok(id) => id,
        Err(IdentityError::Absent) => return CandidateStatus::Absent,
        Err(_) => return CandidateStatus::Uncertain,
    };
    if ident.pid != pid {
        return CandidateStatus::Absent;
    }
    if ident.user_sid != owner.user_sid || ident.session_id != owner.session_id {
        return CandidateStatus::Refused("sid-session");
    }
    let rid = match held.integrity() {
        Ok(r) => r,
        Err(IdentityError::Absent) => return CandidateStatus::Absent,
        Err(_) => return CandidateStatus::Uncertain,
    };
    if !is_medium_rid(rid) {
        return CandidateStatus::Refused("integrity");
    }
    let class = crate::ffi::class_name(hwnd_u64);
    if super::shell_class_excluded(&class) {
        return CandidateStatus::Refused("shell");
    }
    if crate::ffi::parent_of(hwnd_u64) != 0 {
        return CandidateStatus::Refused("parented");
    }
    if crate::ffi::owner_of(hwnd_u64) != 0 {
        return CandidateStatus::Refused("owned-dialog");
    }
    let ex = crate::ffi::window_exstyle(hwnd_u64);
    if ex & WS_EX_TOOLWINDOW != 0 {
        return CandidateStatus::Refused("tool");
    }
    if ex & WS_EX_TOPMOST != 0 && !allow_project_topmost {
        return CandidateStatus::Refused("topmost");
    }
    if ex & WS_EX_NOACTIVATE != 0 {
        return CandidateStatus::Refused("no-activate");
    }
    // Own Settings is an ordinary slotless member (never a dialog);
    // generic unowned `#32770` dialogs still refuse. Owned windows
    // already refused above, so `owned` reads false here.
    if crate::tiling::workspace_dialog_refuses(&class, false, &ident.exe_path, &owner.exe_path) {
        return CandidateStatus::Refused("dialog");
    }
    let Ok(nonce) = get_nonce(hwnd_u64) else {
        return CandidateStatus::Uncertain;
    };
    if nonce.is_some() {
        return CandidateStatus::Refused("already-claimed");
    }
    let visible = crate::ffi::is_visible(hwnd_u64);
    if !visible {
        return CandidateStatus::Refused("not-visible");
    }
    let snap = ProductSnapshot {
        hwnd: hwnd_u64,
        pid,
        process: ident,
        nonce,
        visible,
        iconic: crate::ffi::is_iconic(hwnd_u64),
        maximized: crate::ffi::is_zoomed(hwnd_u64),
        class,
    };
    if crate::ffi::is_zoomed(hwnd_u64) {
        return CandidateStatus::Retained(snap);
    }
    let style = crate::ffi::window_style(hwnd_u64);
    if style & WS_CAPTION == 0 {
        return CandidateStatus::Retained(snap);
    }
    if snap.iconic {
        return CandidateStatus::Retained(snap);
    }
    CandidateStatus::Admissible(snap)
}

/// Managed-workspace admission bound to the stored full member identity
/// plus the stored visible-membership lifetime tag. The live window must
/// equal the stored [`ProcessIdentity`] exactly and still carry the exact
/// member tag (recycled HWNDs refuse as `WrongIdentity` with no writes,
/// including same-process reuse where PID/creation still agree); retained
/// geometry states hide through the managed path. The tag is rechecked
/// after the nonce install so a reuse landing between admission and the
/// install still refuses. Every outcome is typed.
pub fn admit_managed_claim(
    hwnd_u64: u64,
    owner: &ProcessIdentity,
    expected: &ProcessIdentity,
    member_tag: &str,
) -> std::result::Result<WindowIdentity, ManagedAdmitError> {
    admit_managed_claim_for_hide(hwnd_u64, owner, expected, member_tag, false)
}

/// Managed-workspace hide admission with the narrow project-raised
/// topmost exception. When `allow_project_topmost` is true the live
/// keep-above band stays set through admission and the native hide: the
/// band is never cleared merely to hide. Identity, lifetime, style,
/// nonce, and journal gates are otherwise identical; the hide effect
/// itself still verifies claim plus journal before acting with
/// independent recovery via [`hide_managed_claim`]. No ledger changes.
pub fn admit_managed_claim_for_hide(
    hwnd_u64: u64,
    owner: &ProcessIdentity,
    expected: &ProcessIdentity,
    member_tag: &str,
    allow_project_topmost: bool,
) -> std::result::Result<WindowIdentity, ManagedAdmitError> {
    use ManagedAdmitError::{Absent, Refused, Uncertain, WrongIdentity};
    if !crate::workspace_owner::visible_lifetime_ok(
        member_tag,
        read_member_tag(hwnd_u64).as_deref(),
    ) {
        return Err(WrongIdentity);
    }
    let snap = match classify_candidate_for_hide(hwnd_u64, owner, allow_project_topmost) {
        CandidateStatus::Admissible(snap) | CandidateStatus::Retained(snap) => snap,
        CandidateStatus::Absent => return Err(Absent),
        CandidateStatus::Uncertain => return Err(Uncertain),
        CandidateStatus::Refused(code) => return Err(Refused(code)),
    };
    if snap.process != *expected || snap.pid != expected.pid {
        return Err(WrongIdentity);
    }
    if hold_target_verified(snap.pid, &snap.process).is_err() {
        return Err(WrongIdentity);
    }
    let Ok(tag) = install_nonce(hwnd_u64, owner.pid) else {
        return Err(Uncertain);
    };
    let readback_ok = match query_recovery(hwnd_u64, owner) {
        Ok(back) => {
            back.pid == snap.pid
                && back.process == snap.process
                && back.nonce.as_deref() == Some(tag.as_str())
                && back.visible == snap.visible
        }
        Err(_) => false,
    };
    if !readback_ok || !pid_current(hwnd_u64, snap.pid) {
        let _ = remove_nonce_checked(hwnd_u64, &tag, &snap.process, owner);
        return Err(Uncertain);
    }
    if !crate::workspace_owner::visible_lifetime_ok(
        member_tag,
        read_member_tag(hwnd_u64).as_deref(),
    ) {
        let _ = remove_nonce_checked(hwnd_u64, &tag, &snap.process, owner);
        return Err(WrongIdentity);
    }
    Ok(crate::model::WindowIdentity {
        hwnd: hwnd_u64,
        process: snap.process,
        tag,
        kind: WindowClaimKind::Product,
        show: WindowShowState {
            minimized: snap.iconic,
            maximized: snap.maximized,
        },
    })
}

/// Re-hide a known committed claim without a new admission or ledger
/// append: an app external reveal without foreground must return to
/// hidden under the same identity, never re-admitted into the wrong
/// domain. Verifies live process plus exact nonce before and after the
/// posted hide; retired (destroyed/recycled) claims report retired so the
/// caller can drop the ledger note.
pub fn rehide_known_claim(
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
) -> std::result::Result<bool, DynError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
    if claim.kind != WindowClaimKind::Product {
        return Err(err("refuse: not a product claim"));
    }
    let back = match query_recovery(claim.hwnd, owner) {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("absent:") || msg.contains("sid/session mismatch") {
                return Ok(false);
            }
            return Err(err(format!("uncertain: rehide pre-read {msg}")));
        }
    };
    if back.process != claim.process || back.nonce.as_deref() != Some(claim.tag.as_str()) {
        return Ok(false);
    }
    if !back.visible {
        return Ok(true);
    }
    let held = match hold_target_verified(back.pid, &back.process) {
        Ok(h) => h,
        Err(e) if e.to_string().starts_with("absent:") => return Ok(false),
        Err(e) => return Err(err(format!("uncertain: rehide held {e}"))),
    };
    match post_visibility(claim.hwnd, SW_HIDE, false, HIDE_TIMEOUT_MS) {
        Ok(()) => {}
        Err(e) if e.to_string().starts_with("absent:") => return Ok(false),
        Err(e) => return Err(e),
    }
    let _ = &held;
    if !held.is_alive() || !pid_current(claim.hwnd, back.pid) {
        return Err(err("uncertain: rehide post-pid changed"));
    }
    let after = query_recovery(claim.hwnd, owner)?;
    if after.process != claim.process
        || after.nonce.as_deref() != Some(claim.tag.as_str())
        || after.visible
    {
        return Err(err("uncertain: rehide readback mismatch (kept ledger)"));
    }
    Ok(true)
}

pub fn admit_product_target(hwnd_u64: u64, owner: &ProcessIdentity) -> Result<WindowIdentity> {
    let snap = query_candidate(hwnd_u64, owner)?;
    if !snap.visible || snap.iconic {
        return Err(err("refuse: target not visible"));
    }
    if snap.nonce.is_some() {
        return Err(err("refuse: target already claimed"));
    }
    let _held = hold_target_verified(snap.pid, &snap.process)?;
    let tag = install_nonce(hwnd_u64, owner.pid)?;
    let back = query_candidate(hwnd_u64, owner)?;
    if back.pid != snap.pid
        || back.process != snap.process
        || back.nonce.as_deref() != Some(tag.as_str())
        || !back.visible
    {
        let _ = remove_nonce_checked(hwnd_u64, &tag, &snap.process, owner);
        return Err(err("refuse: admit readback mismatch"));
    }
    if !pid_current(hwnd_u64, snap.pid) {
        let _ = remove_nonce_checked(hwnd_u64, &tag, &snap.process, owner);
        return Err(err("refuse: admit pid changed"));
    }
    Ok(crate::model::WindowIdentity {
        hwnd: hwnd_u64,
        process: snap.process,
        tag,
        kind: WindowClaimKind::Product,
        show: WindowShowState::default(),
    })
}

/// Best-effort orphan cleanup for an admitted-but-uncommitted nonce.
/// Removes the window-lifetime nonce ONLY when the fresh committed ledger
/// proves no same-HWND product claim exists AND the live window still
/// carries the full expected process plus the exact nonce. Any ledger
/// with a same-HWND claim, any unreadable ledger, or any process/nonce
/// mismatch retains the residue. Never fails: the caller's error stands.
fn maybe_cleanup_orphan(store: &LedgerStore, owner: &ProcessIdentity, claim: &WindowIdentity) {
    let committed = match store.committed() {
        Ok(Some(record)) => record,
        // Unreadable or absent ledger: retain, never touch the window.
        _ => return,
    };
    if committed.owner != *owner {
        return;
    }
    // Any same-HWND product claim (any tag) blocks removal: a colliding
    // claim retires without foreign writes, never by deleting the nonce.
    if committed
        .windows
        .iter()
        .any(|w| w.hwnd == claim.hwnd && w.kind == WindowClaimKind::Product)
    {
        return;
    }
    let snap = match query_recovery(claim.hwnd, owner) {
        Ok(s) => s,
        // Destroyed/recycled/absent: nonce is gone or not ours.
        Err(_) => return,
    };
    if snap.pid != claim.process.pid
        || snap.process != claim.process
        || snap.nonce.as_deref() != Some(claim.tag.as_str())
    {
        return;
    }
    let _ = remove_nonce_checked(claim.hwnd, &claim.tag, &claim.process, owner);
}

pub fn hide_committed_product(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
) -> Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
    if claim.kind != WindowClaimKind::Product {
        return Err(err("refuse: not a product claim"));
    }
    if !valid_claim_tag(&claim.tag) {
        return Err(err("refuse: bad product tag"));
    }
    if let Err(e) = verify_watcher_ready(dir, owner) {
        maybe_cleanup_orphan(store, owner, claim);
        return Err(e);
    }
    let pre = match query_candidate(claim.hwnd, owner) {
        Ok(s) => s,
        Err(e) => {
            maybe_cleanup_orphan(store, owner, claim);
            return Err(e);
        }
    };
    if pre.pid != claim.process.pid
        || pre.process != claim.process
        || pre.nonce.as_deref() != Some(claim.tag.as_str())
        || !pre.visible
    {
        maybe_cleanup_orphan(store, owner, claim);
        return Err(err("refuse: hide pre-readback mismatch"));
    }
    let held = match hold_target_verified(pre.pid, &pre.process) {
        Ok(h) => h,
        Err(e) => {
            maybe_cleanup_orphan(store, owner, claim);
            return Err(e);
        }
    };
    if let Err(e) = append_product_claim(store, owner, claim) {
        maybe_cleanup_orphan(store, owner, claim);
        return Err(e);
    }
    let _ = &held;
    match post_visibility(claim.hwnd, SW_HIDE, false, HIDE_TIMEOUT_MS) {
        Ok(()) => {}
        Err(e) if e.to_string().starts_with("absent:") => {
            let mut ledger_ms = 0;
            let _ = remove_product_note(store, owner, claim.hwnd, &mut ledger_ms);
            return Err(e);
        }
        Err(e) => return Err(e),
    }
    if !held.is_alive() || !pid_current(claim.hwnd, pre.pid) {
        return Err(err("uncertain: hide post-pid changed"));
    }
    let back = query_recovery(claim.hwnd, owner)?;
    if back.pid != claim.process.pid
        || back.process != claim.process
        || back.nonce.as_deref() != Some(claim.tag.as_str())
        || back.visible
    {
        return Err(err("uncertain: hide readback mismatch (kept ledger)"));
    }
    Ok(())
}

/// Hide an already-managed claim admitted via [`admit_managed_claim`]:
/// the caller holds the full expected process identity plus the exact
/// window-lifetime nonce from that explicit admission, so the fresh
/// pre/post reads are identity-only ([`query_recovery`]) rather than the
/// strict ordinary admission ([`query_candidate`]). Retained
/// minimized/maximized/captionless members admitted as managed hide here;
/// ordinary targets stay on [`admit_product_target`] plus
/// [`hide_committed_product`] with the strict gates. The fresh show state
/// (iconic, maximized) is captured from the pre-hide read and committed
/// to the ledger BEFORE hiding, so independent restore (watcher,
/// standalone, graceful teardown) returns the durable pre-hide state.
/// Returns the committed claim carrying the durable preimage.
pub fn hide_managed_claim(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
) -> Result<WindowIdentity> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
    // Shape refusal takes no cleanup, as before.
    check_claim_shape(claim)?;
    let (pre, held) = match pre_hide_checks(owner, claim, dir) {
        Ok(v) => v,
        Err(e) => {
            maybe_cleanup_orphan(store, owner, claim);
            return Err(e);
        }
    };
    let durable = durable_hide_claim(claim, &pre);
    let mut ledger_ms = 0;
    if let Err(e) = commit_hide_claim(store, owner, &durable, &mut ledger_ms) {
        maybe_cleanup_orphan(store, owner, claim);
        return Err(e);
    }
    let _ = &held;
    match post_visibility(claim.hwnd, SW_HIDE, false, HIDE_TIMEOUT_MS) {
        Ok(()) => {}
        Err(e) if e.to_string().starts_with("absent:") => {
            let _ = remove_product_note(store, owner, claim.hwnd, &mut ledger_ms);
            return Err(e);
        }
        Err(e) => return Err(e),
    }
    finish_hide_confirm(owner, claim, &held, pre.pid)?;
    Ok(durable)
}

fn commit_windows(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    windows: Vec<WindowIdentity>,
) -> Result<()> {
    let committed = store.committed().map_err(store_err)?;
    let Some(existing) = committed else {
        return Err(err("refuse: ledger missing under owner lease"));
    };
    if existing.owner != *owner {
        return Err(err("refuse: ledger owner changed under lease"));
    }
    store
        .commit(&crate::model::RecoveryLedger {
            v: crate::model::LEDGER_SCHEMA_VERSION,
            owner: owner.clone(),
            windows,
            mouse_snap: existing.mouse_snap,
        })
        .map_err(store_err)
}

fn append_product_claim(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
) -> Result<()> {
    let committed = store.committed().map_err(store_err)?;
    let Some(existing) = committed else {
        return Err(err("refuse: ledger missing under owner lease"));
    };
    if existing.owner != *owner {
        return Err(err("refuse: ledger owner changed under lease"));
    }
    if existing.windows.iter().any(|w| w.hwnd == claim.hwnd) {
        return Err(err("refuse: duplicate product hwnd"));
    }
    let mut windows = existing.windows;
    windows.push(claim.clone());
    commit_windows(store, owner, windows)
}

pub fn reveal_product_claim(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
) -> std::result::Result<super::ProductTeardown, DynError> {
    reveal_product_claim_to(store, owner, claim, claim.show.minimized)
}

/// Identity-safe reveal with show-state preservation from the durable
/// ledger preimage. The committed [`WindowShowState`] selects the
/// nonactivating show command (`SW_SHOWMINNOACTIVE` for originally
/// minimized, `SW_SHOWNA` otherwise, which preserves native maximized
/// placement), so a retained member returns to its pre-hide state even on
/// independent restore paths that never saw the hide. The `minimized`
/// parameter carries the in-memory hide-time state for preimage-less
/// (v3) claims; a durable minimized preimage always wins. An
/// already-visible window retires its claim without a write, preserving
/// external drift.
pub fn reveal_product_claim_to(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
) -> std::result::Result<super::ProductTeardown, DynError> {
    use super::ProductTeardown;
    let mut ledger_ms = 0;
    let ready = prepare_reveal(store, owner, claim, minimized, &mut ledger_ms)?;
    let prep = match ready {
        RevealReady::Done(teardown) => return Ok(teardown),
        RevealReady::Post(prep) => prep,
    };
    match post_visibility(claim.hwnd, prep.show_cmd, true, REVEAL_TIMEOUT_MS) {
        Ok(()) => {}
        Err(e) if e.to_string().starts_with("absent:") => {
            remove_product_note(store, owner, claim.hwnd, &mut ledger_ms)?;
            return Ok(ProductTeardown::Retired);
        }
        Err(e) => return Err(e),
    }
    finish_reveal_guarded(
        store,
        owner,
        claim,
        Some((&prep.held, prep.pid)),
        &mut ledger_ms,
    )
}

/// Remove one HWND note from the recovery ledger under the owner lease.
/// Shared by reveal preparation/finish and batch absent recovery. Times
/// only the store commit into `ledger_ms`.
pub(crate) fn remove_product_note(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    hwnd: u64,
    ledger_ms: &mut u64,
) -> Result<()> {
    let committed = store.committed().map_err(store_err)?;
    let Some(existing) = committed else {
        return Ok(());
    };
    if existing.owner != *owner {
        return Err(err("refuse: ledger owner changed under lease"));
    }
    let t0 = std::time::Instant::now();
    let result = commit_windows(
        store,
        owner,
        existing
            .windows
            .into_iter()
            .filter(|w| w.hwnd != hwnd)
            .collect(),
    );
    *ledger_ms =
        ledger_ms.saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    result
}

pub fn reveal_all_product(
    store: &LedgerStore,
    owner: &ProcessIdentity,
) -> (Vec<super::ProductTeardown>, Vec<WindowIdentity>) {
    let claims: Vec<WindowIdentity> = store
        .committed()
        .ok()
        .flatten()
        .map(|r| {
            r.windows
                .into_iter()
                .filter(|w| w.kind == WindowClaimKind::Product)
                .collect()
        })
        .unwrap_or_default();
    let mut outcomes = Vec::new();
    let mut uncertain = Vec::new();
    for claim in &claims {
        match reveal_product_claim(store, owner, claim) {
            Ok(o) => outcomes.push(o),
            Err(_) => {
                outcomes.push(super::ProductTeardown::Uncertain);
                uncertain.push(claim.clone());
            }
        }
    }
    (outcomes, uncertain)
}

/// Offline native-window fake for joint-batch tests (test-only, never
/// compiled into production). Scripts the leaf OS reads/writes behind the
/// shared prepare/post/finish above - HWND liveness, PID/process binding,
/// product nonce and member-tag properties, `ShowWindowAsync` posts plus
/// visibility, and the target hold - while the phase state machine, the
/// ledger, and the outcome vocabulary stay real. Production logic never
/// branches on this: gates live in the leaf fns and fall through to the OS
/// for every unregistered HWND, so all existing tests (which never register)
/// keep their honest native-failure outcomes.
#[cfg(test)]
pub(crate) mod fake {
    use super::ProductSnapshot;
    use crate::model::ProcessIdentity;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// One scripted native window. `process` is trusted test input (no
    /// SID/session/integrity re-proof: the fake stands in for the OS reads,
    /// not for the gates that consume them).
    pub(crate) struct FakeWindow {
        pub pid: u32,
        pub process: ProcessIdentity,
        /// Scripted target integrity RID standing in for the OS token
        /// read: the fake-branch gates consume it with the real
        /// medium predicate, never the ambient runner level.
        pub integrity_rid: u32,
        pub nonce: Option<String>,
        pub member_tag: Option<String>,
        pub visible: bool,
        pub iconic: bool,
        pub maximized: bool,
        pub alive: bool,
        /// Scripted `ShowWindowAsync` verdict: `false` simulates a refused
        /// native post with no visibility change.
        pub post_ok: bool,
        /// When true, posts report success but the host never applies the
        /// effect (visibility flips only through explicit test flips):
        /// proves the joint wait, not the post, confirms.
        pub defer: bool,
    }

    thread_local! {
        static WINDOWS: RefCell<HashMap<u64, FakeWindow>> = RefCell::new(HashMap::new());
        /// Posted effects in call order: `(hwnd, hide)`.
        static POST_LOG: RefCell<Vec<(u64, bool)>> = const { RefCell::new(Vec::new()) };
        /// Visibility snapshot reads in call order (confirmations only).
        static SNAPSHOT_LOG: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
        /// Combined post/snapshot order: `('p' | 's', hwnd)`.
        static OP_LOG: RefCell<Vec<(char, u64)>> = const { RefCell::new(Vec::new()) };
        /// Scripted float-intent marker slots by HWND: present entry is a
        /// present property value, absent entry is no property. Stands in
        /// for the window property table for registered HWNDs only, so
        /// marker install/peek roundtrips run offline; unregistered HWNDs
        /// fall through to the OS exactly as before.
        static FLOAT_MARKS: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
    }

    pub(crate) fn register(hwnd: u64, window: FakeWindow) {
        WINDOWS.with(|map| map.borrow_mut().insert(hwnd, window));
    }

    pub(crate) fn clear() {
        WINDOWS.with(|map| map.borrow_mut().clear());
        POST_LOG.with(|log| log.borrow_mut().clear());
        SNAPSHOT_LOG.with(|log| log.borrow_mut().clear());
        OP_LOG.with(|log| log.borrow_mut().clear());
        FLOAT_MARKS.with(|map| map.borrow_mut().clear());
    }

    pub(crate) fn any_registered() -> bool {
        WINDOWS.with(|map| !map.borrow().is_empty())
    }

    pub(crate) fn active(hwnd: u64) -> bool {
        WINDOWS.with(|map| map.borrow().contains_key(&hwnd))
    }

    pub(crate) fn with<R>(hwnd: u64, f: impl FnOnce(&FakeWindow) -> R) -> Option<R> {
        WINDOWS.with(|map| map.borrow().get(&hwnd).map(f))
    }

    pub(crate) fn with_mut<R>(hwnd: u64, f: impl FnOnce(&mut FakeWindow) -> R) -> Option<R> {
        WINDOWS.with(|map| map.borrow_mut().get_mut(&hwnd).map(f))
    }

    /// Scripted float-intent marker read: `None` falls through to the OS
    /// (unregistered HWND); `Some` mirrors the property table (`Some`
    /// value present, `None` absent, including destroyed fakes whose
    /// markers die with the window).
    pub(crate) fn peek_float_mark(hwnd: u64) -> Option<Option<u64>> {
        if !active(hwnd) {
            return None;
        }
        Some(FLOAT_MARKS.with(|map| map.borrow().get(&hwnd).copied()))
    }

    /// Scripted float-intent marker write: `false` falls through to the
    /// OS (unregistered HWND) or mirrors a failed `SetProp` (destroyed
    /// fake); `true` stores and reads back through the same table.
    pub(crate) fn store_float_mark(hwnd: u64, value: u64) -> bool {
        if !active(hwnd) {
            return false;
        }
        if with(hwnd, |w| w.alive).unwrap_or(false) {
            FLOAT_MARKS.with(|map| map.borrow_mut().insert(hwnd, value));
            true
        } else {
            false
        }
    }

    pub(crate) fn post_log() -> Vec<(u64, bool)> {
        POST_LOG.with(|log| log.borrow().clone())
    }

    pub(crate) fn snapshot_log() -> Vec<u64> {
        SNAPSHOT_LOG.with(|log| log.borrow().clone())
    }

    pub(crate) fn record_post(hwnd: u64, hide: bool) {
        POST_LOG.with(|log| log.borrow_mut().push((hwnd, hide)));
        OP_LOG.with(|log| log.borrow_mut().push(('p', hwnd)));
    }

    pub(crate) fn record_snapshot(hwnd: u64) {
        SNAPSHOT_LOG.with(|log| log.borrow_mut().push(hwnd));
        OP_LOG.with(|log| log.borrow_mut().push(('s', hwnd)));
    }

    pub(crate) fn op_log() -> Vec<(char, u64)> {
        OP_LOG.with(|log| log.borrow().clone())
    }

    pub(crate) fn snapshot(hwnd: u64) -> Option<ProductSnapshot> {
        with(hwnd, |w| ProductSnapshot {
            hwnd,
            pid: w.pid,
            process: w.process.clone(),
            nonce: w.nonce.clone(),
            visible: w.visible,
            iconic: w.iconic,
            maximized: w.maximized,
            class: String::new(),
        })
        .filter(|_| with(hwnd, |w| w.alive).unwrap_or(false))
    }

    /// Test-only target hold: validates pid/process binding against the fake
    /// and returns a self-process handle as the liveness guard stand-in
    /// (its `is_alive` reads true; death is simulated through the fake
    /// `alive` flag plus `pid_current`, which the shared finish checks
    /// first). Production never takes this path.
    pub(crate) fn hold_verified(
        pid: u32,
        expected: &ProcessIdentity,
    ) -> Option<super::Result<super::HeldProcess>> {
        if !any_registered() {
            return None;
        }
        let owned: Vec<(u64, u32, ProcessIdentity, bool)> = WINDOWS.with(|map| {
            map.borrow()
                .iter()
                .map(|(h, w)| (*h, w.pid, w.process.clone(), w.alive))
                .collect()
        });
        let Some((_, _, live, alive)) = owned.iter().find(|(_, p, _, _)| *p == pid) else {
            return Some(Err(super::err("absent: target pid absent")));
        };
        if !alive || live != expected {
            return Some(Err(super::err(
                "absent: target identity changed (recycled)",
            )));
        }
        let current = crate::ffi::current_process_id();
        match super::HeldProcess::open(current) {
            Ok(held) => Some(Ok(held)),
            Err(e) => Some(Err(super::err(format!("error: target held {e}")))),
        }
    }
}
