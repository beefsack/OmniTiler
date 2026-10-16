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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn prop_name() -> Vec<u16> {
    wide(PRODUCT_CLAIM_PROP)
}

/// Fresh read of the product nonce. `Ok(None)` means verifiably absent: a
/// recycled HWND starts without it.
fn get_nonce(
    hwnd: windows_sys::Win32::Foundation::HWND,
) -> std::result::Result<Option<String>, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetPropW;
    let name = prop_name();
    let v = unsafe { GetPropW(hwnd, name.as_ptr()) };
    if v.is_null() {
        return Ok(None);
    }
    let token = v as usize as u64;
    if token == 0 {
        return Ok(None);
    }
    Ok(Some(format!("{token:016x}")))
}

fn class_of(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_OWNER, GWL_EXSTYLE, GetParent, GetWindow, GetWindowLongW, IsIconic, IsWindow,
        IsWindowVisible, IsZoomed, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    };
    if hwnd_u64 == 0 {
        return Err(err("refuse: zero hwnd"));
    }
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err(err("absent: hwnd destroyed"));
    }
    let mut pid: u32 = 0;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);
    }
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
    let class = class_of(hwnd);
    if super::shell_class_excluded(&class) {
        return Err(err("refuse: shell window"));
    }
    if !unsafe { GetParent(hwnd) }.is_null() {
        return Err(err("refuse: parented"));
    }
    if !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null() {
        return Err(err("refuse: owned dialog"));
    }
    let ex = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
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
        use windows_sys::Win32::UI::WindowsAndMessaging::{GWL_STYLE, IsZoomed, WS_CAPTION};
        if unsafe { IsZoomed(hwnd) } != 0 {
            return Err(err("refuse: fullscreen"));
        }
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
        if style & WS_CAPTION == 0 {
            return Err(err("refuse: borderless (possible fullscreen)"));
        }
    }
    let nonce = get_nonce(hwnd).map_err(|e| err(format!("error: nonce read {e}")))?;
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    let iconic = unsafe { IsIconic(hwnd) } != 0;
    Ok(ProductSnapshot {
        hwnd: hwnd_u64,
        pid,
        process: ident,
        nonce,
        visible,
        iconic,
        maximized: unsafe { IsZoomed(hwnd) } != 0,
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, IsWindow, IsWindowVisible, IsZoomed,
    };
    if hwnd_u64 == 0 {
        return Err(err("refuse: zero hwnd"));
    }
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err(err("absent: hwnd destroyed"));
    }
    let mut pid: u32 = 0;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);
    }
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
    let nonce = get_nonce(hwnd).map_err(|e| err(format!("error: nonce read {e}")))?;
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    let iconic = unsafe { IsIconic(hwnd) } != 0;
    Ok(ProductSnapshot {
        hwnd: hwnd_u64,
        pid,
        process: ident,
        nonce,
        visible,
        iconic,
        maximized: unsafe { IsZoomed(hwnd) } != 0,
        class: String::new(),
    })
}

/// Install a fresh random nonzero window-lifetime nonce, then read it
/// back. Only call after [`query_candidate`] admitted the window; the
/// caller commits the returned tag BEFORE hiding.
pub fn install_nonce(hwnd_u64: u64, owner_pid: u32) -> Result<String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetPropW;
    let hwnd = hwnd_u64 as isize as HWND;
    for _ in 0..4 {
        let tag = generate_claim_tag(owner_pid);
        debug_assert!(valid_claim_tag(&tag));
        let token = u64::from_str_radix(&tag, 16).map_err(|_| err("error: nonce render"))?;
        if token == 0 {
            continue;
        }
        let name = prop_name();
        let ok = unsafe { SetPropW(hwnd, name.as_ptr(), token as usize as _) };
        if ok == 0 {
            return Err(err("error: SetProp failed"));
        }
        match get_nonce(hwnd).map_err(|e| err(format!("error: nonce readback {e}")))? {
            Some(back) if back == tag => return Ok(tag),
            _ => {
                let _ = remove_nonce_inner(hwnd);
                return Err(err("error: nonce readback mismatch"));
            }
        }
    }
    Err(err("error: nonce install failed"))
}

fn remove_nonce_inner(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::RemovePropW;
    let name = prop_name();
    !unsafe { RemovePropW(hwnd, name.as_ptr()) }.is_null()
        || get_nonce(hwnd).ok().flatten().is_none()
}

fn member_prop_name() -> Vec<u16> {
    wide(MEMBER_TAG_PROP)
}

/// Fresh read of the visible-membership lifetime tag. `None` means
/// verifiably no tag: a same-process HWND reuse starts without our
/// window-lifetime property, and a destroyed HWND reads back nothing.
/// Read-only; never mutates the window.
pub fn read_member_tag(hwnd_u64: u64) -> Option<String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetPropW;
    let hwnd = hwnd_u64 as isize as HWND;
    let name = member_prop_name();
    let v = unsafe { GetPropW(hwnd, name.as_ptr()) };
    if v.is_null() {
        return None;
    }
    let token = v as usize as u64;
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetPropW;
    let hwnd = hwnd_u64 as isize as HWND;
    for _ in 0..4 {
        let tag = generate_claim_tag(owner_pid);
        debug_assert!(valid_claim_tag(&tag));
        let Ok(token) = u64::from_str_radix(&tag, 16) else {
            return Err(err("error: member tag render"));
        };
        if token == 0 {
            continue;
        }
        let name = member_prop_name();
        let ok = unsafe { SetPropW(hwnd, name.as_ptr(), token as usize as _) };
        if ok == 0 {
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

fn sticky_prop_name() -> Vec<u16> {
    wide(STICKY_PROP)
}

fn float_intent_prop_name() -> Vec<u16> {
    wide(FLOAT_INTENT_PROP)
}

fn tile_override_prop_name() -> Vec<u16> {
    wide(TILE_OVERRIDE_PROP)
}

/// One honest `GetPropW` read of our own property name. The `GetPropW`
/// contract promises NULL when the list does not contain the string and
/// promises no last-error distinction, so NULL reads as
/// [`MarkerSlot::Absent`] unconditionally: absent reads on real windows
/// report a nonzero error (e.g. `ERROR_FILE_NOT_FOUND`), which must
/// never diagnose as [`MarkerSlot::Unreadable`]. A destroyed HWND reads
/// absent (the marker dies with the window). Same convention as
/// [`get_nonce`], [`read_member_tag`], and the helper lifetime read.
fn read_prop_slot(hwnd_u64: u64, name: &[u16]) -> MarkerSlot {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetPropW, IsWindow};
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return MarkerSlot::Absent;
    }
    let v = unsafe { GetPropW(hwnd, name.as_ptr()) };
    if v.is_null() {
        MarkerSlot::Absent
    } else {
        // Non-NULL handles are nonzero; zero is unrepresentable here
        // (NULL reads absent above), so `Value(0)` never escapes.
        MarkerSlot::Value(v as usize as u64)
    }
}

/// Sticky-float marker slot: absent is silent no-intent, a wrong value
/// is logged no-intent, a failed read is diagnosed no-intent.
/// Read-only. A recycled HWND starts without our property.
pub fn peek_sticky_raw(hwnd_u64: u64) -> MarkerSlot {
    read_prop_slot(hwnd_u64, &sticky_prop_name())
}

/// Intentional-float marker slot. Same honest absences as
/// [`peek_sticky_raw`].
pub fn peek_float_intent_raw(hwnd_u64: u64) -> MarkerSlot {
    read_prop_slot(hwnd_u64, &float_intent_prop_name())
}

/// Reserved tile-override marker slot (item 13 D7). No admission path
/// reads it in item 8.
#[allow(dead_code)]
pub fn peek_tile_override_raw(hwnd_u64: u64) -> MarkerSlot {
    read_prop_slot(hwnd_u64, &tile_override_prop_name())
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetPropW;
    let hwnd = hwnd_u64 as isize as HWND;
    let value = crate::tiling::sticky_marker_value(prior_floating);
    let name = sticky_prop_name();
    let ok = unsafe { SetPropW(hwnd, name.as_ptr(), value as usize as _) };
    if ok == 0 {
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, RemovePropW};
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
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
    let name = sticky_prop_name();
    let _ = unsafe { RemovePropW(hwnd, name.as_ptr()) };
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetPropW;
    let hwnd = hwnd_u64 as isize as HWND;
    let value = crate::tiling::float_intent_marker_value();
    let name = float_intent_prop_name();
    let ok = unsafe { SetPropW(hwnd, name.as_ptr(), value as usize as _) };
    if ok == 0 {
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetPropW;
    let hwnd = hwnd_u64 as isize as HWND;
    let value = crate::tiling::tile_override_marker_value();
    let name = tile_override_prop_name();
    let ok = unsafe { SetPropW(hwnd, name.as_ptr(), value as usize as _) };
    if ok == 0 {
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, RemovePropW};
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return Ok(());
    }
    match peek_float_intent_raw(hwnd_u64) {
        MarkerSlot::Absent => return Ok(()),
        MarkerSlot::Unreadable => {
            return Err(err("error: float intent marker read error"));
        }
        MarkerSlot::Value(_) => {}
    }
    let name = float_intent_prop_name();
    let _ = unsafe { RemovePropW(hwnd, name.as_ptr()) };
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, RemovePropW};
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return Ok(());
    }
    match peek_tile_override_raw(hwnd_u64) {
        MarkerSlot::Absent => return Ok(()),
        MarkerSlot::Unreadable => {
            return Err(err("error: tile override marker read error"));
        }
        MarkerSlot::Value(_) => {}
    }
    let name = tile_override_prop_name();
    let _ = unsafe { RemovePropW(hwnd, name.as_ptr()) };
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
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
    let _ = remove_nonce_inner(hwnd);
    let _ = &held;
    if !held.is_alive() || !pid_current(hwnd_u64, snap.pid) {
        return Err(err("uncertain: nonce remove post-pid changed"));
    }
    match get_nonce(hwnd).map_err(|e| err(format!("error: nonce remove readback {e}")))? {
        None => Ok(true),
        Some(_) => Err(err("error: nonce remove failed")),
    }
}

fn pump_once() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
    };
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Post one visibility effect without blocking, then pump until the
/// readback observes it. `ShowWindowAsync` queues behind earlier posts so
/// hide/show ordering per window is preserved, a hung target cannot wedge
/// the caller, and no orphan setter thread outlives the call. Success
/// requires the observed effect, never the post alone.
fn post_visibility(hwnd_u64: u64, cmd: i32, expect_visible: bool, timeout_ms: u64) -> Result<()> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, IsWindowVisible, ShowWindowAsync};
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { ShowWindowAsync(hwnd, cmd) } == 0 {
        return Err(err("error: visibility post failed"));
    }
    let start = Instant::now();
    let budget = Duration::from_millis(timeout_ms);
    loop {
        pump_once();
        if unsafe { IsWindow(hwnd) } == 0 {
            return Err(err("absent: hwnd destroyed"));
        }
        if (unsafe { IsWindowVisible(hwnd) } != 0) == expect_visible {
            return Ok(());
        }
        if start.elapsed() >= budget {
            return Err(err("uncertain: visibility not observed"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub fn hold_target_verified(pid: u32, expected: &ProcessIdentity) -> Result<HeldProcess> {
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut current: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd_u64 as isize as HWND, &mut current);
    }
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetParent, GetWindow, GetWindowLongW, IsIconic, IsWindow,
        IsWindowVisible, IsZoomed, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    };
    if hwnd_u64 == 0 {
        return CandidateStatus::Absent;
    }
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return CandidateStatus::Absent;
    }
    let mut pid: u32 = 0;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);
    }
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
    let class = class_of(hwnd);
    if super::shell_class_excluded(&class) {
        return CandidateStatus::Refused("shell");
    }
    if !unsafe { GetParent(hwnd) }.is_null() {
        return CandidateStatus::Refused("parented");
    }
    if !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null() {
        return CandidateStatus::Refused("owned-dialog");
    }
    let ex = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
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
    let Ok(nonce) = get_nonce(hwnd) else {
        return CandidateStatus::Uncertain;
    };
    if nonce.is_some() {
        return CandidateStatus::Refused("already-claimed");
    }
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    if !visible {
        return CandidateStatus::Refused("not-visible");
    }
    let snap = ProductSnapshot {
        hwnd: hwnd_u64,
        pid,
        process: ident,
        nonce,
        visible,
        iconic: unsafe { IsIconic(hwnd) } != 0,
        maximized: unsafe { IsZoomed(hwnd) } != 0,
        class,
    };
    if unsafe { IsZoomed(hwnd) } != 0 {
        return CandidateStatus::Retained(snap);
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
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
            let _ = remove_product_note(store, owner, claim.hwnd);
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
    let pre = match query_recovery(claim.hwnd, owner) {
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
    let durable = WindowIdentity {
        show: WindowShowState {
            minimized: pre.iconic,
            maximized: pre.maximized,
        },
        ..claim.clone()
    };
    if let Err(e) = append_product_claim(store, owner, &durable) {
        maybe_cleanup_orphan(store, owner, claim);
        return Err(e);
    }
    let _ = &held;
    match post_visibility(claim.hwnd, SW_HIDE, false, HIDE_TIMEOUT_MS) {
        Ok(()) => {}
        Err(e) if e.to_string().starts_with("absent:") => {
            let _ = remove_product_note(store, owner, claim.hwnd);
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
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, SW_SHOWMINNOACTIVE, SW_SHOWNA};
    if claim.kind != WindowClaimKind::Product {
        return Err(err("refuse: not a product claim"));
    }
    let hwnd = claim.hwnd;
    if unsafe { IsWindow(hwnd as isize as _) } == 0 {
        remove_product_note(store, owner, hwnd)?;
        return Ok(ProductTeardown::Retired);
    }
    let snap = match query_recovery(hwnd, owner) {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("absent:") {
                remove_product_note(store, owner, hwnd)?;
                return Ok(ProductTeardown::Retired);
            }
            if msg.contains("sid/session mismatch") {
                remove_product_note(store, owner, hwnd)?;
                return Ok(ProductTeardown::Retired);
            }
            return Err(err(format!("uncertain: reveal pre-read {msg}")));
        }
    };
    if snap.process != claim.process || snap.nonce.as_deref() != Some(claim.tag.as_str()) {
        remove_product_note(store, owner, hwnd)?;
        return Ok(ProductTeardown::Retired);
    }
    if snap.visible {
        remove_nonce_checked(hwnd, &claim.tag, &claim.process, owner)?;
        remove_product_note(store, owner, hwnd)?;
        return Ok(ProductTeardown::AlreadyVisible);
    }
    let held = match hold_target_verified(snap.pid, &snap.process) {
        Ok(h) => h,
        Err(e) if e.to_string().starts_with("absent:") => {
            remove_product_note(store, owner, hwnd)?;
            return Ok(ProductTeardown::Retired);
        }
        Err(e) => return Err(err(format!("uncertain: reveal held {e}"))),
    };
    let show_cmd = match product_show_restore(WindowShowState {
        minimized: claim.show.minimized || minimized,
        maximized: claim.show.maximized,
    }) {
        ProductShowRestore::ShowMinimized => SW_SHOWMINNOACTIVE,
        ProductShowRestore::ShowNormal | ProductShowRestore::ShowMaximized => SW_SHOWNA,
    };
    match post_visibility(hwnd, show_cmd, true, REVEAL_TIMEOUT_MS) {
        Ok(()) => {}
        Err(e) if e.to_string().starts_with("absent:") => {
            remove_product_note(store, owner, hwnd)?;
            return Ok(ProductTeardown::Retired);
        }
        Err(e) => return Err(e),
    }
    let _ = &held;
    if !held.is_alive() || !pid_current(hwnd, snap.pid) {
        return Err(err("uncertain: reveal post-pid changed"));
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
    remove_product_note(store, owner, hwnd)?;
    Ok(ProductTeardown::Revealed)
}

fn remove_product_note(store: &LedgerStore, owner: &ProcessIdentity, hwnd: u64) -> Result<()> {
    let committed = store.committed().map_err(store_err)?;
    let Some(existing) = committed else {
        return Ok(());
    };
    if existing.owner != *owner {
        return Err(err("refuse: ledger owner changed under lease"));
    }
    commit_windows(
        store,
        owner,
        existing
            .windows
            .into_iter()
            .filter(|w| w.hwnd != hwnd)
            .collect(),
    )
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
