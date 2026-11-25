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
use crate::native_guards::MarkerSlot;
use crate::storage::{LEDGER_FILE_NAME, LedgerStore};
use crate::tiling_sys::IntentStorage;
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

// Test-only admission write-seam proof (offline): counts entries into
// `install_member_tag`, the single native write the admission preamble can
// perform (`SetProp` of our own member-tag property, fail-closed). Reads in
// `admit_observed` never touch this seam. Production behavior is unchanged:
// the counter is never read outside tests.
#[cfg(test)]
thread_local! {
    static INSTALL_TAG_ATTEMPTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Reset the member-tag install-attempt counter (test-only).
#[cfg(test)]
pub fn reset_install_tag_attempts() {
    INSTALL_TAG_ATTEMPTS.with(|slot| slot.set(0));
}

/// Read the member-tag install-attempt counter (test-only).
#[cfg(test)]
#[must_use]
pub fn install_tag_attempts() -> u64 {
    INSTALL_TAG_ATTEMPTS.with(|slot| slot.get())
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
    #[cfg(test)]
    INSTALL_TAG_ATTEMPTS.with(|slot| slot.set(slot.get() + 1));
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

// R12 Task 8: one packed, lifetime-bound intent slot on the window.
//
// The three legacy `OmniTiler.*.v1` properties were written sequentially
// with no rollback. The packed slot moves all three intents in one native
// mutation, and binds the value to the exact live window lifetime (PID plus
// the FNV-1a hash over the process creation and member tag) so a recycled
// HWND or a same-process reuse can never read a foreign value as its own.
//
// Layout of the encoded value (stored as `encoded + 1`, so a stored zero is
// never valid):
// - bits 0-1: sticky prior code (0 none, 1 prior false, 2 prior true, 3 invalid)
// - bit 2: float intent
// - bit 3: tile override
// - bits 4-7: reserved, must read zero
// - bits 8-39: window PID (32 bits)
// - bits 40-63: low 24 bits of FNV-1a over `process_creation` then the tag
const INTENT_SLOT_PROP: &str = "OmniTiler.IntentSlot.v2";
const INTENT_STICKY_MASK: u64 = 0b11;
const INTENT_FLOAT_BIT: u64 = 1 << 2;
const INTENT_TILE_BIT: u64 = 1 << 3;
const INTENT_RESERVED_MASK: u64 = 0xf0;
const INTENT_PID_SHIFT: u32 = 8;
const INTENT_PID_MASK: u64 = 0xffff_ffff << INTENT_PID_SHIFT;
const INTENT_HASH_SHIFT: u32 = 40;
const INTENT_HASH_MASK: u64 = 0x00ff_ffff << INTENT_HASH_SHIFT;

/// One decoded intent-slot transition: set or clear exactly one lane while
/// preserving every other bit. Used by every marker helper so a single
/// packed write carries the whole resulting storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntentChange {
    SetSticky(bool),
    ClearSticky,
    SetFloat,
    ClearFloat,
    SetTile,
    ClearTile,
}

/// Removal verdict for one verified storage read before a sticky-off: only
/// the live prior value the caller remembers may be cleared. A different
/// value owns the window now and is never touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StickyClearVerdict {
    NotOwned,
    AlreadyAbsent,
    Clear,
}

fn apply_intent_change(storage: IntentStorage, change: IntentChange) -> IntentStorage {
    let mut next = storage;
    match change {
        IntentChange::SetSticky(prior) => next.sticky_prior = Some(prior),
        IntentChange::ClearSticky => next.sticky_prior = None,
        IntentChange::SetFloat => next.float = true,
        IntentChange::ClearFloat => next.float = false,
        IntentChange::SetTile => next.tile = true,
        IntentChange::ClearTile => next.tile = false,
    }
    next
}

fn sticky_clear_verdict(current: &IntentStorage, expected_prior: bool) -> StickyClearVerdict {
    match current.sticky_prior {
        None => StickyClearVerdict::AlreadyAbsent,
        Some(prior) if prior == expected_prior => StickyClearVerdict::Clear,
        Some(_) => StickyClearVerdict::NotOwned,
    }
}

/// 32-bit FNV-1a over `process_creation` bytes followed by the tag bytes,
/// truncated to the low 24 bits. Deliberately dependency-free: the hash is
/// only a lifetime discriminator, never a security boundary.
fn intent_slot_hash(process_creation: &str, tag: &str) -> u32 {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET;
    for byte in process_creation
        .as_bytes()
        .iter()
        .chain(tag.as_bytes())
    {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash & 0x00ff_ffff
}

fn intent_storage_empty(storage: &IntentStorage) -> bool {
    storage.sticky_prior.is_none() && !storage.float && !storage.tile
}

/// Encode one storage value into the packed slot's *stored* value
/// (`encoded + 1`). `None` only on the representational overflow no valid
/// bit pattern can reach; callers fail closed instead of wrapping.
pub(crate) fn encode_intent_slot(
    storage: &IntentStorage,
    pid: u32,
    process_creation: &str,
    tag: &str,
) -> Option<usize> {
    let sticky_code = match storage.sticky_prior {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    let mut encoded = sticky_code;
    if storage.float {
        encoded |= INTENT_FLOAT_BIT;
    }
    if storage.tile {
        encoded |= INTENT_TILE_BIT;
    }
    encoded |= u64::from(pid) << INTENT_PID_SHIFT;
    encoded |= u64::from(intent_slot_hash(process_creation, tag)) << INTENT_HASH_SHIFT;
    // A stored zero is never a valid slot, so every encoding adds one.
    usize::try_from(encoded.checked_add(1)?).ok()
}

/// Decode one stored slot value for the live lifetime. `None` covers every
/// corrupt or foreign-lifetime shape: stored zero, an invalid sticky code,
/// a stray reserved bit, a different PID, or a different
/// creation/tag hash. Callers read that as unreadable, never as absent.
pub(crate) fn decode_intent_slot(
    stored: usize,
    pid: u32,
    process_creation: &str,
    tag: &str,
) -> Option<IntentStorage> {
    let encoded = u64::try_from(stored).ok()?.checked_sub(1)?;
    if encoded & INTENT_RESERVED_MASK != 0 {
        return None;
    }
    let sticky_prior = match encoded & INTENT_STICKY_MASK {
        0 => None,
        1 => Some(false),
        2 => Some(true),
        _ => return None,
    };
    if encoded & INTENT_PID_MASK != u64::from(pid) << INTENT_PID_SHIFT {
        return None;
    }
    if encoded & INTENT_HASH_MASK
        != u64::from(intent_slot_hash(process_creation, tag)) << INTENT_HASH_SHIFT
    {
        return None;
    }
    Some(IntentStorage {
        sticky_prior,
        float: encoded & INTENT_FLOAT_BIT != 0,
        tile: encoded & INTENT_TILE_BIT != 0,
    })
}

/// Resolve the exact live lifetime the packed slot binds to: the window's
/// live PID, that process's creation stamp, and the live member tag. Any
/// unverifiable read returns `None`; a slot that cannot be bound to a live
/// lifetime is never trusted.
fn intent_slot_identity(hwnd: u64) -> Option<(u32, String, String)> {
    if !crate::ffi::is_window(hwnd) {
        return None;
    }
    let pid = crate::ffi::window_pid(hwnd);
    if pid == 0 {
        return None;
    }
    let held = HeldProcess::open(pid).ok()?;
    let identity = held.identity().ok()?;
    if identity.pid != pid {
        return None;
    }
    let tag = read_member_tag(hwnd).filter(|tag| !tag.is_empty())?;
    Some((pid, identity.process_creation, tag))
}

/// Read the packed slot: `None` when the property is present but
/// unverifiable (corrupt value, or bound to a different lifetime), never as
/// absent. `Some(None)` is the canonical no-intent state (property absent),
/// and `Some(Some(storage))` is a value proven against the live lifetime.
pub(crate) fn read_intent_slot(hwnd: u64) -> Option<Option<IntentStorage>> {
    if !crate::ffi::is_window(hwnd) {
        // Properties die with the window: nothing can be present to trust.
        return Some(None);
    }
    let Some(stored) = crate::ffi::get_prop(hwnd, INTENT_SLOT_PROP) else {
        return Some(None);
    };
    let (pid, creation, tag) = intent_slot_identity(hwnd)?;
    decode_intent_slot(stored, pid, &creation, &tag).map(Some)
}

/// Remove the obsolete per-property `.v1` markers once the packed slot has
/// superseded them, so an empty-slot transition can never resurrect stale
/// pre-upgrade intent through the legacy fallback. Returns whether every
/// legacy name reads verifiably absent afterwards.
fn retire_legacy_markers(hwnd: u64) -> bool {
    [STICKY_PROP, FLOAT_INTENT_PROP, TILE_OVERRIDE_PROP]
        .into_iter()
        .all(|name| {
            if crate::ffi::get_prop(hwnd, name).is_some() {
                crate::ffi::remove_prop(hwnd, name);
            }
            crate::ffi::get_prop(hwnd, name).is_none()
        })
}

/// Commit one whole packed storage value with exactly one slot mutation.
///
/// Empty storage retires the legacy per-property residue first and then
/// removes the packed slot, so the slot removal is the single atomic commit
/// and the old value stays readable if either step fails. Non-empty storage
/// performs one `set` and proves the readback. Split from the native calls so
/// the transition order is testable with scripted effects.
fn commit_intent_slot(
    storage: IntentStorage,
    identity: (u32, &str, &str),
    set: impl FnOnce(usize) -> bool,
    remove: impl FnOnce(),
    retire_legacy: impl FnOnce() -> bool,
    read: impl FnOnce() -> Option<Option<IntentStorage>>,
) -> std::result::Result<(), ()> {
    let (pid, process_creation, tag) = identity;
    if intent_storage_empty(&storage) {
        if !retire_legacy() {
            return Err(());
        }
        remove();
        return match read() {
            Some(None) => Ok(()),
            _ => Err(()),
        };
    }
    let stored = encode_intent_slot(&storage, pid, process_creation, tag).ok_or(())?;
    if !set(stored) {
        return Err(());
    }
    match read() {
        Some(Some(back)) if back == storage => Ok(()),
        _ => Err(()),
    }
}

/// Write one whole intent storage into the packed lifetime-bound slot and
/// prove the readback. One slot mutation runs per transition: one `SetProp`
/// for non-empty storage, or one `RemoveProp` for the canonical empty state.
/// An empty transition first retires any pre-upgrade `.v1` residue while the
/// packed slot still shadows it, so clearing intent cannot resurrect stale
/// legacy values and the slot removal stays the single commit point.
pub(crate) fn write_intent_slot(hwnd: u64, storage: IntentStorage) -> std::result::Result<(), ()> {
    #[cfg(test)]
    if fake::marker_active(hwnd) {
        return fake_write_intent_slot(hwnd, storage);
    }
    let Some((pid, creation, tag)) = intent_slot_identity(hwnd) else {
        return Err(());
    };
    commit_intent_slot(
        storage,
        (pid, &creation, &tag),
        |stored| crate::ffi::set_prop(hwnd, INTENT_SLOT_PROP, stored),
        || crate::ffi::remove_prop(hwnd, INTENT_SLOT_PROP),
        || retire_legacy_markers(hwnd),
        || read_intent_slot(hwnd),
    )
}

/// Test-only packed-slot write over the scripted marker tables: a synthetic
/// HWND has no real lifetime to bind, so the fake lane applies every lane to
/// the same tables [`peek_sticky_raw`]/[`peek_float_intent_raw`]/
/// [`peek_tile_override_raw`] read, then proves the readback against those
/// values. Production semantics are untouched; every store/remove must
/// report success or the transition fails.
#[cfg(test)]
fn fake_write_intent_slot(hwnd: u64, storage: IntentStorage) -> std::result::Result<(), ()> {
    let sticky_ok = match storage.sticky_prior {
        Some(prior) => fake::store_sticky_mark(hwnd, crate::tiling::sticky_marker_value(prior)),
        None => fake::remove_sticky_mark(hwnd),
    };
    let float_ok = if storage.float {
        fake::store_float_mark(hwnd, crate::tiling::float_intent_marker_value())
    } else {
        fake::remove_float_mark(hwnd)
    };
    let tile_ok = if storage.tile {
        fake::store_tile_mark(hwnd, crate::tiling::tile_override_marker_value())
    } else {
        fake::remove_tile_mark(hwnd)
    };
    let sticky_back = fake::peek_sticky_mark(hwnd).flatten()
        == storage.sticky_prior.map(crate::tiling::sticky_marker_value);
    let float_back =
        fake::peek_float_mark(hwnd).flatten().is_some() == storage.float;
    let tile_back = fake::peek_tile_mark(hwnd).flatten().is_some() == storage.tile;
    if sticky_ok && float_ok && tile_ok && sticky_back && float_back && tile_back {
        Ok(())
    } else {
        Err(())
    }
}

/// Canonical storage for one helper update: the verified packed slot when it
/// is present, else the legacy per-property install (adopted so pre-existing
/// markers keep working). `Err` when any live read is unreadable.
fn current_intent_storage(hwnd: u64) -> std::result::Result<(IntentStorage, bool), ()> {
    match read_intent_slot(hwnd) {
        None => Err(()),
        Some(Some(storage)) => Ok((storage, true)),
        Some(None) => legacy_intent_storage(hwnd)
            .map(|storage| (storage, false))
            .ok_or(()),
    }
}

/// Decode the legacy per-property installs into one storage value. A wrong
/// legacy value reads as no intent for that lane (the pre-slot contract);
/// an unreadable read fails the whole decode.
fn legacy_intent_storage(hwnd: u64) -> Option<IntentStorage> {
    let sticky_prior = match read_prop_slot(hwnd, STICKY_PROP) {
        MarkerSlot::Absent => None,
        MarkerSlot::Value(value) => crate::tiling::parse_sticky_marker(value),
        MarkerSlot::Unreadable => return None,
    };
    let float = matches!(
        read_prop_slot(hwnd, FLOAT_INTENT_PROP),
        MarkerSlot::Value(value)
            if crate::tiling::parse_float_intent_marker(value).is_some()
    );
    let tile = matches!(
        read_prop_slot(hwnd, TILE_OVERRIDE_PROP),
        MarkerSlot::Value(value)
            if crate::tiling::parse_tile_override_marker(value).is_some()
    );
    Some(IntentStorage {
        sticky_prior,
        float,
        tile,
    })
}

/// Marker-slot projection for one lane: the packed slot wins when verified
/// (an unset bit is canonical absence, never a legacy fallback), a corrupt
/// or foreign slot is unreadable, and only an absent slot falls back to the
/// legacy property so pre-existing installs are still adopted.
fn packed_marker_or_legacy(
    hwnd: u64,
    legacy_prop: &str,
    pick: impl FnOnce(&IntentStorage) -> MarkerSlot,
) -> MarkerSlot {
    match read_intent_slot(hwnd) {
        None => MarkerSlot::Unreadable,
        Some(Some(storage)) => pick(&storage),
        Some(None) => read_prop_slot(hwnd, legacy_prop),
    }
}

/// Sticky-float marker slot: absent is silent no-intent, a wrong value
/// is logged no-intent, a failed read is diagnosed no-intent.
/// Read-only. A recycled HWND starts without our property. The packed
/// intent slot wins when present; only an absent slot falls back to the
/// legacy `.v1` property.
pub fn peek_sticky_raw(hwnd_u64: u64) -> MarkerSlot {
    #[cfg(test)]
    if let Some(slot) = fake::peek_sticky_mark(hwnd_u64) {
        return match slot {
            Some(value) => MarkerSlot::Value(value),
            None => MarkerSlot::Absent,
        };
    }
    packed_marker_or_legacy(hwnd_u64, STICKY_PROP, |storage| {
        storage.sticky_prior.map_or(MarkerSlot::Absent, |prior| {
            MarkerSlot::Value(crate::tiling::sticky_marker_value(prior))
        })
    })
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
    packed_marker_or_legacy(hwnd_u64, FLOAT_INTENT_PROP, |storage| {
        if storage.float {
            MarkerSlot::Value(crate::tiling::float_intent_marker_value())
        } else {
            MarkerSlot::Absent
        }
    })
}

/// Reserved tile-override marker slot (item 13 D7). No admission path
/// reads it in item 8.
#[allow(dead_code)]
pub fn peek_tile_override_raw(hwnd_u64: u64) -> MarkerSlot {
    #[cfg(test)]
    if let Some(slot) = fake::peek_tile_mark(hwnd_u64) {
        return match slot {
            Some(value) => MarkerSlot::Value(value),
            None => MarkerSlot::Absent,
        };
    }
    packed_marker_or_legacy(hwnd_u64, TILE_OVERRIDE_PROP, |storage| {
        if storage.tile {
            MarkerSlot::Value(crate::tiling::tile_override_marker_value())
        } else {
            MarkerSlot::Absent
        }
    })
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
    #[cfg(test)]
    if fake::marker_active(hwnd_u64) {
        let value = crate::tiling::sticky_marker_value(prior_floating);
        if !fake::store_sticky_mark(hwnd_u64, value) {
            return Err(err("error: SetProp failed"));
        }
        return match peek_sticky_raw(hwnd_u64) {
            MarkerSlot::Value(back)
                if crate::tiling::parse_sticky_marker(back) == Some(prior_floating) =>
            {
                Ok(())
            }
            MarkerSlot::Unreadable => Err(err("error: sticky marker read error")),
            _ => Err(err("error: sticky marker readback mismatch")),
        };
    }
    let (storage, _) = current_intent_storage(hwnd_u64)
        .map_err(|_| err("error: sticky marker read error"))?;
    let desired = apply_intent_change(storage, IntentChange::SetSticky(prior_floating));
    write_intent_slot(hwnd_u64, desired).map_err(|_| err("error: SetProp failed"))?;
    match peek_sticky_raw(hwnd_u64) {
        MarkerSlot::Value(back)
            if crate::tiling::parse_sticky_marker(back) == Some(prior_floating) =>
        {
            Ok(())
        }
        MarkerSlot::Unreadable => Err(err("error: sticky marker read error")),
        _ => Err(err("error: sticky marker readback mismatch")),
    }
}

/// Remove the sticky marker only when the live value still equals the
/// expected pre-sticky state. Returns `true` when verifiably absent
/// afterwards (removed or already absent), `false` when a different value
/// owns the window now (never touch it). A failed read errors without
/// mutating. Identity fencing stays with the caller.
pub fn remove_sticky_marker(hwnd_u64: u64, expected_prior: bool) -> Result<bool> {
    #[cfg(test)]
    if fake::marker_active(hwnd_u64) {
        match peek_sticky_raw(hwnd_u64) {
            MarkerSlot::Absent => return Ok(true),
            MarkerSlot::Unreadable => return Err(err("error: sticky marker read error")),
            MarkerSlot::Value(value) => {
                if crate::tiling::parse_sticky_marker(value) != Some(expected_prior) {
                    return Ok(false);
                }
            }
        }
        fake::remove_sticky_mark(hwnd_u64);
        return match peek_sticky_raw(hwnd_u64) {
            MarkerSlot::Absent => Ok(true),
            MarkerSlot::Unreadable => Err(err("error: sticky marker read error")),
            MarkerSlot::Value(_) => Err(err("error: sticky marker remove failed")),
        };
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return Ok(true);
    }
    let (mut storage, packed) = match read_intent_slot(hwnd_u64) {
        None => return Err(err("error: sticky marker read error")),
        Some(Some(storage)) => (storage, true),
        Some(None) => match legacy_intent_storage(hwnd_u64) {
            Some(storage) => (storage, false),
            None => return Err(err("error: sticky marker read error")),
        },
    };
    let verdict = if packed {
        sticky_clear_verdict(&storage, expected_prior)
    } else {
        // Legacy-only install: the live pre-slot property decides exactly as
        // before, so a wrong or corrupt value is never touched.
        match read_prop_slot(hwnd_u64, STICKY_PROP) {
            MarkerSlot::Absent => StickyClearVerdict::AlreadyAbsent,
            MarkerSlot::Unreadable => return Err(err("error: sticky marker read error")),
            MarkerSlot::Value(value) => match crate::tiling::parse_sticky_marker(value) {
                Some(prior) if prior == expected_prior => {
                    storage.sticky_prior = Some(prior);
                    StickyClearVerdict::Clear
                }
                _ => StickyClearVerdict::NotOwned,
            },
        }
    };
    match verdict {
        StickyClearVerdict::NotOwned => return Ok(false),
        StickyClearVerdict::AlreadyAbsent => return Ok(true),
        StickyClearVerdict::Clear => {}
    }
    let desired = apply_intent_change(storage, IntentChange::ClearSticky);
    write_intent_slot(hwnd_u64, desired)
        .map_err(|_| err("error: sticky marker remove failed"))?;
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
    #[cfg(test)]
    if fake::marker_active(hwnd_u64) {
        let value = crate::tiling::float_intent_marker_value();
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
    let (storage, _) = current_intent_storage(hwnd_u64)
        .map_err(|_| err("error: float intent marker read error"))?;
    let desired = apply_intent_change(storage, IntentChange::SetFloat);
    write_intent_slot(hwnd_u64, desired).map_err(|_| err("error: SetProp failed"))?;
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
    #[cfg(test)]
    if fake::marker_active(hwnd_u64) {
        let value = crate::tiling::tile_override_marker_value();
        if !fake::store_tile_mark(hwnd_u64, value) {
            return Err(err("error: SetProp failed"));
        }
        return match peek_tile_override_raw(hwnd_u64) {
            MarkerSlot::Value(back)
                if crate::tiling::parse_tile_override_marker(back).is_some() =>
            {
                Ok(())
            }
            MarkerSlot::Unreadable => Err(err("error: tile override marker read error")),
            _ => Err(err("error: tile override marker readback mismatch")),
        };
    }
    let (storage, _) = current_intent_storage(hwnd_u64)
        .map_err(|_| err("error: tile override marker read error"))?;
    let desired = apply_intent_change(storage, IntentChange::SetTile);
    write_intent_slot(hwnd_u64, desired).map_err(|_| err("error: SetProp failed"))?;
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
    #[cfg(test)]
    if fake::marker_active(hwnd_u64) {
        fake::remove_float_mark(hwnd_u64);
        return match peek_float_intent_raw(hwnd_u64) {
            MarkerSlot::Absent => Ok(()),
            MarkerSlot::Unreadable => Err(err("error: float intent marker read error")),
            MarkerSlot::Value(_) => Err(err("error: float intent marker remove failed")),
        };
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return Ok(());
    }
    let (storage, packed) = current_intent_storage(hwnd_u64)
        .map_err(|_| err("error: float intent marker read error"))?;
    if !storage.float {
        // Canonical absence needs no write. Legacy-only corrupt residue on
        // our own property name still cleans here, as it always did.
        if packed || crate::ffi::get_prop(hwnd_u64, FLOAT_INTENT_PROP).is_none() {
            return Ok(());
        }
        crate::ffi::remove_prop(hwnd_u64, FLOAT_INTENT_PROP);
        return match read_prop_slot(hwnd_u64, FLOAT_INTENT_PROP) {
            MarkerSlot::Absent => Ok(()),
            MarkerSlot::Unreadable | MarkerSlot::Value(_) => {
                Err(err("error: float intent marker remove failed"))
            }
        };
    }
    let desired = apply_intent_change(storage, IntentChange::ClearFloat);
    write_intent_slot(hwnd_u64, desired)
        .map_err(|_| err("error: float intent marker remove failed"))?;
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
    #[cfg(test)]
    if fake::marker_active(hwnd_u64) {
        fake::remove_tile_mark(hwnd_u64);
        return match peek_tile_override_raw(hwnd_u64) {
            MarkerSlot::Absent => Ok(()),
            MarkerSlot::Unreadable => Err(err("error: tile override marker read error")),
            MarkerSlot::Value(_) => Err(err("error: tile override marker remove failed")),
        };
    }
    if !crate::ffi::is_window(hwnd_u64) {
        return Ok(());
    }
    let (storage, packed) = current_intent_storage(hwnd_u64)
        .map_err(|_| err("error: tile override marker read error"))?;
    if !storage.tile {
        if packed || crate::ffi::get_prop(hwnd_u64, TILE_OVERRIDE_PROP).is_none() {
            return Ok(());
        }
        crate::ffi::remove_prop(hwnd_u64, TILE_OVERRIDE_PROP);
        return match read_prop_slot(hwnd_u64, TILE_OVERRIDE_PROP) {
            MarkerSlot::Absent => Ok(()),
            MarkerSlot::Unreadable | MarkerSlot::Value(_) => {
                Err(err("error: tile override marker remove failed"))
            }
        };
    }
    let desired = apply_intent_change(storage, IntentChange::ClearTile);
    write_intent_slot(hwnd_u64, desired)
        .map_err(|_| err("error: tile override marker remove failed"))?;
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

/// Post one visibility effect, then pump until the readback observes it.
fn post_visibility(hwnd_u64: u64, cmd: i32, expect_visible: bool, timeout_ms: u64) -> Result<()> {
    post_visibility_traced(hwnd_u64, cmd, expect_visible, timeout_ms, None)
}

/// Traced visibility post for the world route: records the actual host API
/// that ran (never the configured mode) into `sink` when armed. `None`
/// keeps the legacy behavior exactly (no record, no clock).
fn post_visibility_traced(
    hwnd_u64: u64,
    cmd: i32,
    expect_visible: bool,
    timeout_ms: u64,
    sink: Option<&mut VisibilityPostTrace>,
) -> Result<()> {
    if !post_async_traced(hwnd_u64, cmd, sink) {
        return Err(err("error: visibility post failed"));
    }
    poll_visibility(hwnd_u64, expect_visible, timeout_ms, true)
}

/// Pump-free nonactivating visibility post for remembered-focus view
/// switches. Reuses the existing budgets and sleep cadence.
#[allow(dead_code)]
fn post_visibility_pos(
    hwnd_u64: u64,
    hide: bool,
    expect_visible: bool,
    timeout_ms: u64,
) -> Result<()> {
    post_visibility_pos_traced(hwnd_u64, hide, expect_visible, timeout_ms, None)
}

/// Traced nonactivating post (see [`post_visibility_traced`]).
fn post_visibility_pos_traced(
    hwnd_u64: u64,
    hide: bool,
    expect_visible: bool,
    timeout_ms: u64,
    sink: Option<&mut VisibilityPostTrace>,
) -> Result<()> {
    if !post_async_pos_traced(hwnd_u64, hide, sink) {
        return Err(err("error: visibility post failed"));
    }
    poll_visibility(hwnd_u64, expect_visible, timeout_ms, false)
}

/// ---- Joint hide/reveal primitives (select/send/rollback) ---- ///
/// Shared prepare/post/finish behind the single-window
/// [`hide_managed_claim`] / [`reveal_product_claim_to`] below, so paths
/// cannot diverge in safety: identical commit-before-hide, identical
/// identity/nonce/ledger ownership, identical failure recovery. Single
/// paths keep the per-window pumped bound.
///
/// A prepared hide owns its target through the returned
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

/// Visibility actuation mode: pumped `ShowWindowAsync` versus
/// nonactivating `SetWindowPos` for remembered-focus pure views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VisibilityMode {
    PumpedShowAsync,
    NonactivatingPos,
}

/// Actual nonactivating `SetWindowPos` flag words posted by
/// [`post_async_pos`] (hide `0x4097`, show `0x4057`): `SWP_NOMOVE |
/// SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS` plus
/// `SWP_HIDEWINDOW` (hide) or `SWP_SHOWWINDOW` (show). Test-pinned below
/// against the composed `SWP_*` expression so a flag change breaks the
/// build instead of silently desyncing diagnostics vocabulary.
pub(crate) const NA_POS_HIDE_FLAGS: u32 = 0x4097;
pub(crate) const NA_POS_SHOW_FLAGS: u32 = 0x4057;

/// Trace-only host API vocabulary for visibility posts: the exact
/// primitive that ran. Pumped hides/reveals post `ShowWindowAsync` with
/// the show command; nonactivating view hides/reveals post `SetWindowPos`
/// with the async flag word (`SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER |
/// SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS` plus `SWP_HIDEWINDOW`/`SWP_SHOWWINDOW`).
/// Recorded only at the actual post site when the caller arms a sink;
/// early vetos (admission, ledger, pre-read) record nothing and never
/// infer a call from the configured mode. No user content: HWND plus
/// the mechanical call facts only.
pub(crate) const VIEW_POST_API_POS: &str = "SetWindowPos";
pub(crate) const VIEW_POST_API_SHOW: &str = "ShowWindowAsync";

/// Trace-only record of one executed visibility post: which host API ran
/// for which HWND and direction, with the activation-affecting parameter
/// (async `SetWindowPos` flag word or `ShowWindowAsync` command), the
/// post verdict, and stamps strictly around the primitive on the shared
/// log `mono_ms` basis. Call-to-return alone never proves arrival; the
/// existing visibility readback does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibilityPostDiag {
    pub(crate) hwnd: u64,
    pub(crate) hide: bool,
    pub(crate) api: &'static str,
    pub(crate) flags: Option<u32>,
    pub(crate) cmd: Option<i32>,
    pub(crate) posted: bool,
    pub(crate) call_mono_ms: u64,
    pub(crate) ret_mono_ms: u64,
}

/// Bound for retained executed-visibility-post rows per pass. The
/// production posts themselves are never capped (actuation runs every
/// hide/reveal); only this diagnostic storage caps, with the unbounded
/// total below keeping the world-apply omission honest.
pub(crate) const MAX_VISIBILITY_POSTS: usize = 8;

/// Bounded trace accumulator for executed visibility posts: the actual
/// host API records from the post sites, in execution order. `total`
/// counts every executed post including rows past the storage cap, so
/// the tick diag derives an honest omitted count instead of a false
/// zero. Never a policy input; arrivals still prove only through the
/// visibility readback.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct VisibilityPostTrace {
    pub(crate) rows: Vec<VisibilityPostDiag>,
    pub(crate) total: usize,
}

impl VisibilityPostTrace {
    pub(crate) fn push(&mut self, row: VisibilityPostDiag) {
        self.total = self.total.saturating_add(1);
        if self.rows.len() < MAX_VISIBILITY_POSTS {
            self.rows.push(row);
        }
    }

    pub(crate) fn take(&mut self) -> (Vec<VisibilityPostDiag>, usize) {
        (
            std::mem::take(&mut self.rows),
            std::mem::take(&mut self.total),
        )
    }
}

/// Async visibility post. Returns `false` when the post refused.
#[allow(dead_code)]
fn post_async(hwnd_u64: u64, cmd: i32) -> bool {
    post_async_traced(hwnd_u64, cmd, None)
}

/// Traced async post (see [`post_visibility_traced`]): stamps and records
/// the actual `ShowWindowAsync` call when `sink` is armed, including
/// refusals; unarmed runs skip the record and the clock.
fn post_async_traced(hwnd_u64: u64, cmd: i32, sink: Option<&mut VisibilityPostTrace>) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
    let hide = cmd == SW_HIDE;
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            fake::record_post(hwnd_u64, hide, true);
            let posted = fake::with_mut(hwnd_u64, |w| {
                if !w.alive || !w.post_ok {
                    return false;
                }
                if !w.defer {
                    w.visible = !hide;
                }
                true
            })
            .unwrap_or(false);
            if let Some(trace) = sink {
                let call_mono_ms = crate::tiling_sys::trace::log_mono_ms();
                let ret_mono_ms = crate::tiling_sys::trace::log_mono_ms();
                trace.push(VisibilityPostDiag {
                    hwnd: hwnd_u64,
                    hide,
                    api: VIEW_POST_API_SHOW,
                    flags: None,
                    cmd: Some(cmd),
                    posted,
                    call_mono_ms,
                    ret_mono_ms,
                });
            }
            return posted;
        }
    }
    let armed = sink.is_some();
    let call_mono_ms = if armed {
        crate::tiling_sys::trace::log_mono_ms()
    } else {
        0
    };
    let posted = crate::ffi::show_window_async(hwnd_u64, cmd);
    if let Some(rows) = sink {
        rows.push(VisibilityPostDiag {
            hwnd: hwnd_u64,
            hide,
            api: VIEW_POST_API_SHOW,
            flags: None,
            cmd: Some(cmd),
            posted,
            call_mono_ms,
            ret_mono_ms: crate::tiling_sys::trace::log_mono_ms(),
        });
    }
    posted
}

/// Nonactivating async visibility post via `SetWindowPos` with
/// `SWP_ASYNCWINDOWPOS` (hide `0x4097`, show `0x4057`). The caller never
/// pumps, so edges the wait would deliver stay undelivered during the wait.
#[allow(dead_code)]
fn post_async_pos(hwnd_u64: u64, hide: bool) -> bool {
    post_async_pos_traced(hwnd_u64, hide, None)
}

/// Traced nonactivating post (see [`post_visibility_traced`]): stamps and
/// records the actual `SetWindowPos` flag word when `sink` is armed.
fn post_async_pos_traced(
    hwnd_u64: u64,
    hide: bool,
    sink: Option<&mut VisibilityPostTrace>,
) -> bool {
    #[cfg(test)]
    {
        if fake::active(hwnd_u64) {
            fake::record_post(hwnd_u64, hide, false);
            let posted = fake::with_mut(hwnd_u64, |w| {
                if !w.alive || !w.post_ok {
                    return false;
                }
                if !w.defer {
                    w.visible = !hide;
                }
                true
            })
            .unwrap_or(false);
            if let Some(trace) = sink {
                let call_mono_ms = crate::tiling_sys::trace::log_mono_ms();
                let ret_mono_ms = crate::tiling_sys::trace::log_mono_ms();
                trace.push(VisibilityPostDiag {
                    hwnd: hwnd_u64,
                    hide,
                    api: VIEW_POST_API_POS,
                    flags: Some(if hide {
                        NA_POS_HIDE_FLAGS
                    } else {
                        NA_POS_SHOW_FLAGS
                    }),
                    cmd: None,
                    posted,
                    call_mono_ms,
                    ret_mono_ms,
                });
            }
            return posted;
        }
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_ASYNCWINDOWPOS, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SWP_SHOWWINDOW,
    };
    let flags = SWP_NOMOVE
        | SWP_NOSIZE
        | SWP_NOZORDER
        | SWP_NOACTIVATE
        | SWP_ASYNCWINDOWPOS
        | if hide { SWP_HIDEWINDOW } else { SWP_SHOWWINDOW };
    debug_assert_eq!(
        flags,
        if hide {
            NA_POS_HIDE_FLAGS
        } else {
            NA_POS_SHOW_FLAGS
        },
        "nonactivating SetWindowPos flag word changed: update NA_POS_*_FLAGS"
    );
    let armed = sink.is_some();
    let call_mono_ms = if armed {
        crate::tiling_sys::trace::log_mono_ms()
    } else {
        0
    };
    let posted = crate::ffi::set_window_pos(hwnd_u64, 0, 0, 0, 0, flags);
    if let Some(rows) = sink {
        rows.push(VisibilityPostDiag {
            hwnd: hwnd_u64,
            hide,
            api: VIEW_POST_API_POS,
            flags: Some(flags),
            cmd: None,
            posted,
            call_mono_ms,
            ret_mono_ms: crate::tiling_sys::trace::log_mono_ms(),
        });
    }
    posted
}

/// Shared visibility poll. Reuses the existing budgets and sleep cadence.
fn poll_visibility(hwnd_u64: u64, expect_visible: bool, timeout_ms: u64, pump: bool) -> Result<()> {
    let start = Instant::now();
    let budget = Duration::from_millis(timeout_ms);
    loop {
        if pump {
            pump_once();
        }
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

/// Fake-aware HWND liveness for the prepare path: the scripted `alive`
/// flag offline, `IsWindow` in production. Production never inspects
/// fabricated HWNDs.
fn live_hwnd(hwnd_u64: u64) -> bool {
    #[cfg(test)]
    if fake::active(hwnd_u64) {
        return fake::with(hwnd_u64, |w| w.alive).unwrap_or(false);
    }
    crate::ffi::is_window(hwnd_u64)
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

/// Fresh visibility readback for one window: `Some(visible)` on a
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

/// Post-hide finish for one window: the post-pid fence plus the
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
    if !crate::native_guards::visible_lifetime_ok(member_tag, read_member_tag(hwnd_u64).as_deref())
    {
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
    if !crate::native_guards::visible_lifetime_ok(member_tag, read_member_tag(hwnd_u64).as_deref())
    {
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
    hide_managed_claim_with_mode(store, owner, claim, dir, VisibilityMode::PumpedShowAsync)
}

/// Mode-parameterized hide. Only the visibility post differs.
pub(crate) fn hide_managed_claim_with_mode(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
    mode: VisibilityMode,
) -> Result<WindowIdentity> {
    hide_managed_claim_with_mode_traced(store, owner, claim, dir, mode, None)
}

/// Traced hide for the world route: the actual visibility post records
/// into `sink` when armed (see [`post_visibility_traced`]). Admission,
/// ledger, and readback behavior are identical; early vetos record
/// nothing.
pub(crate) fn hide_managed_claim_with_mode_traced(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
    mode: VisibilityMode,
    sink: Option<&mut VisibilityPostTrace>,
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
    let post_result = match mode {
        VisibilityMode::PumpedShowAsync => {
            post_visibility_traced(claim.hwnd, SW_HIDE, false, HIDE_TIMEOUT_MS, sink)
        }
        VisibilityMode::NonactivatingPos => {
            post_visibility_pos_traced(claim.hwnd, true, false, HIDE_TIMEOUT_MS, sink)
        }
    };
    match post_result {
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

/// Nonactivating hide for remembered-focus pure-view switches.
pub fn hide_managed_claim_nonactivating(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
) -> Result<WindowIdentity> {
    hide_managed_claim_with_mode(store, owner, claim, dir, VisibilityMode::NonactivatingPos)
}

/// Traced nonactivating hide for the world route (see
/// [`hide_managed_claim_with_mode_traced`]).
pub(crate) fn hide_managed_claim_nonactivating_traced(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
    sink: Option<&mut VisibilityPostTrace>,
) -> Result<WindowIdentity> {
    hide_managed_claim_with_mode_traced(
        store,
        owner,
        claim,
        dir,
        VisibilityMode::NonactivatingPos,
        sink,
    )
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
    reveal_product_claim_to_with_mode(
        store,
        owner,
        claim,
        minimized,
        VisibilityMode::PumpedShowAsync,
    )
}

/// Mode-parameterized reveal sharing all safety with
/// [`reveal_product_claim_to`]. Pumped posts the durable show command;
/// nonactivating posts `SetWindowPos` show, except a minimized restore
/// keeps the pumped `SW_SHOWMINNOACTIVE` post to preserve exact semantics.
pub(crate) fn reveal_product_claim_to_with_mode(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
    mode: VisibilityMode,
) -> std::result::Result<super::ProductTeardown, DynError> {
    reveal_product_claim_to_with_mode_traced(store, owner, claim, minimized, mode, None)
}

/// Traced reveal for the world route: the actual visibility post records
/// into `sink` when armed (see [`post_visibility_traced`]). A minimized
/// restore keeps the pumped `SW_SHOWMINNOACTIVE` post even in the
/// nonactivating mode, and the record names the API that actually ran.
/// Preparation short-circuits (retired/already-visible) record nothing.
pub(crate) fn reveal_product_claim_to_with_mode_traced(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
    mode: VisibilityMode,
    sink: Option<&mut VisibilityPostTrace>,
) -> std::result::Result<super::ProductTeardown, DynError> {
    use super::ProductTeardown;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWMINNOACTIVE;
    let mut ledger_ms = 0;
    let ready = prepare_reveal(store, owner, claim, minimized, &mut ledger_ms)?;
    let prep = match ready {
        RevealReady::Done(teardown) => return Ok(teardown),
        RevealReady::Post(prep) => prep,
    };
    let post_result = match mode {
        VisibilityMode::PumpedShowAsync => {
            post_visibility_traced(claim.hwnd, prep.show_cmd, true, REVEAL_TIMEOUT_MS, sink)
        }
        VisibilityMode::NonactivatingPos if prep.show_cmd == SW_SHOWMINNOACTIVE => {
            post_visibility_traced(claim.hwnd, prep.show_cmd, true, REVEAL_TIMEOUT_MS, sink)
        }
        VisibilityMode::NonactivatingPos => {
            post_visibility_pos_traced(claim.hwnd, false, true, REVEAL_TIMEOUT_MS, sink)
        }
    };
    match post_result {
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

/// Nonactivating reveal for remembered-focus pure-view switches.
pub fn reveal_product_claim_to_nonactivating(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
) -> std::result::Result<super::ProductTeardown, DynError> {
    reveal_product_claim_to_with_mode(
        store,
        owner,
        claim,
        minimized,
        VisibilityMode::NonactivatingPos,
    )
}

/// Traced nonactivating reveal for the world route (see
/// [`reveal_product_claim_to_with_mode_traced`]).
pub(crate) fn reveal_product_claim_to_nonactivating_traced(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
    sink: Option<&mut VisibilityPostTrace>,
) -> std::result::Result<super::ProductTeardown, DynError> {
    reveal_product_claim_to_with_mode_traced(
        store,
        owner,
        claim,
        minimized,
        VisibilityMode::NonactivatingPos,
        sink,
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
        /// Post dispatch in call order: `(hwnd, hide, pumped)`.
        static POST_MODE_LOG: RefCell<Vec<(u64, bool, bool)>> = const { RefCell::new(Vec::new()) };
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
        /// Scripted sticky-float marker slots by HWND (same shape as
        /// [`FLOAT_MARKS`]): the sticky property stores the prior-float
        /// value the durable intent record carries.
        static STICKY_MARKS: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
        /// Scripted tile-override marker slots by HWND (same shape as
        /// [`FLOAT_MARKS`]); presence means the explicit tile win owns the
        /// window.
        static TILE_MARKS: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
        /// HWNDs with scripted marker slots only: the rest of the native
        /// surface stays with the caller's primitives. Lets fixtures that
        /// script everything else through their own tables still exercise
        /// the marker install/read-back lane offline.
        static MARK_ONLY: RefCell<std::collections::HashSet<u64>> =
            RefCell::new(std::collections::HashSet::new());
    }

    pub(crate) fn any_registered() -> bool {
        WINDOWS.with(|map| !map.borrow().is_empty())
    }

    /// Test-only registration of one scripted native window behind the
    /// shared hide/reveal/admit paths. The phase state machine, the ledger,
    /// and the outcome vocabulary stay real; only the leaf OS reads/writes
    /// consult the script. No production caller registers.
    pub(crate) fn register(hwnd: u64, window: FakeWindow) {
        WINDOWS.with(|map| map.borrow_mut().insert(hwnd, window));
    }

    /// Test-only teardown for [`register`]: drops every scripted window so
    /// later tests on this thread observe the bare OS again.
    pub(crate) fn unregister_all() {
        WINDOWS.with(|map| map.borrow_mut().clear());
        FLOAT_MARKS.with(|map| map.borrow_mut().clear());
        STICKY_MARKS.with(|map| map.borrow_mut().clear());
        TILE_MARKS.with(|map| map.borrow_mut().clear());
        MARK_ONLY.with(|set| set.borrow_mut().clear());
        POST_LOG.with(|log| log.borrow_mut().clear());
        POST_MODE_LOG.with(|log| log.borrow_mut().clear());
        SNAPSHOT_LOG.with(|log| log.borrow_mut().clear());
        OP_LOG.with(|log| log.borrow_mut().clear());
    }

    pub(crate) fn active(hwnd: u64) -> bool {
        WINDOWS.with(|map| map.borrow().contains_key(&hwnd))
    }

    /// Test-only marker-only registration: the HWND owns scripted marker
    /// slots without standing in for any other OS read. Fixtures script
    /// the rest of the native surface through their own tables.
    pub(crate) fn register_marks(hwnd: u64) {
        MARK_ONLY.with(|set| {
            set.borrow_mut().insert(hwnd);
        });
    }

    pub(crate) fn clear_marks() {
        MARK_ONLY.with(|set| set.borrow_mut().clear());
    }

    /// Scripted marker slots exist for full fakes and marker-only
    /// registrations alike.
    pub(crate) fn marker_active(hwnd: u64) -> bool {
        active(hwnd) || MARK_ONLY.with(|set| set.borrow().contains(&hwnd))
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
        if !marker_active(hwnd) {
            return None;
        }
        Some(FLOAT_MARKS.with(|map| map.borrow().get(&hwnd).copied()))
    }

    /// Scripted float-intent marker write: `false` falls through to the
    /// OS (unregistered HWND) or mirrors a failed `SetProp` (destroyed
    /// fake); `true` stores and reads back through the same table.
    pub(crate) fn store_float_mark(hwnd: u64, value: u64) -> bool {
        store_mark(&FLOAT_MARKS, hwnd, value)
    }

    /// Scripted sticky-float marker read: same contract as
    /// [`Self::peek_float_mark`].
    pub(crate) fn peek_sticky_mark(hwnd: u64) -> Option<Option<u64>> {
        if !marker_active(hwnd) {
            return None;
        }
        Some(STICKY_MARKS.with(|map| map.borrow().get(&hwnd).copied()))
    }

    /// Scripted sticky-float marker write: same contract as
    /// [`Self::store_float_mark`].
    pub(crate) fn store_sticky_mark(hwnd: u64, value: u64) -> bool {
        store_mark(&STICKY_MARKS, hwnd, value)
    }

    /// Scripted tile-override marker read: same contract as
    /// [`Self::peek_float_mark`].
    pub(crate) fn peek_tile_mark(hwnd: u64) -> Option<Option<u64>> {
        if !marker_active(hwnd) {
            return None;
        }
        Some(TILE_MARKS.with(|map| map.borrow().get(&hwnd).copied()))
    }

    /// Scripted tile-override marker write: same contract as
    /// [`Self::store_float_mark`].
    pub(crate) fn store_tile_mark(hwnd: u64, value: u64) -> bool {
        store_mark(&TILE_MARKS, hwnd, value)
    }

    /// Scripted marker removal: `true` when verifiably absent afterwards
    /// (removed or already absent), `false` for an unregistered HWND so
    /// the caller falls through to the OS.
    pub(crate) fn remove_sticky_mark(hwnd: u64) -> bool {
        remove_mark(&STICKY_MARKS, hwnd)
    }

    pub(crate) fn remove_tile_mark(hwnd: u64) -> bool {
        remove_mark(&TILE_MARKS, hwnd)
    }

    pub(crate) fn remove_float_mark(hwnd: u64) -> bool {
        remove_mark(&FLOAT_MARKS, hwnd)
    }

    fn store_mark(
        table: &'static std::thread::LocalKey<RefCell<HashMap<u64, u64>>>,
        hwnd: u64,
        value: u64,
    ) -> bool {
        if !marker_active(hwnd) {
            return false;
        }
        let alive = if active(hwnd) {
            with(hwnd, |w| w.alive).unwrap_or(false)
        } else {
            // Marker-only registrations are test-trusted live slots.
            true
        };
        if alive {
            table.with(|map| map.borrow_mut().insert(hwnd, value));
            true
        } else {
            false
        }
    }

    fn remove_mark(
        table: &'static std::thread::LocalKey<RefCell<HashMap<u64, u64>>>,
        hwnd: u64,
    ) -> bool {
        if !marker_active(hwnd) {
            return false;
        }
        table.with(|map| map.borrow_mut().remove(&hwnd));
        true
    }

    pub(crate) fn record_post(hwnd: u64, hide: bool, pumped: bool) {
        POST_LOG.with(|log| log.borrow_mut().push((hwnd, hide)));
        POST_MODE_LOG.with(|log| log.borrow_mut().push((hwnd, hide, pumped)));
        OP_LOG.with(|log| log.borrow_mut().push(('p', hwnd)));
    }

    pub(crate) fn post_modes() -> Vec<(u64, bool, bool)> {
        POST_MODE_LOG.with(|log| log.borrow().clone())
    }

    pub(crate) fn record_snapshot(hwnd: u64) {
        SNAPSHOT_LOG.with(|log| log.borrow_mut().push(hwnd));
        OP_LOG.with(|log| log.borrow_mut().push(('s', hwnd)));
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

#[cfg(test)]
mod mode_tests {
    use super::fake::{self, FakeWindow};
    use super::{
        INTENT_FLOAT_BIT, INTENT_TILE_BIT, IntentChange, StickyClearVerdict, VisibilityMode,
        apply_intent_change, commit_intent_slot, decode_intent_slot, encode_intent_slot,
        hide_managed_claim_with_mode, reveal_product_claim_to_with_mode, sticky_clear_verdict,
    };
    use crate::model::{
        LEDGER_SCHEMA_VERSION, ProcessIdentity, RecoveryLedger, WindowClaimKind, WindowIdentity,
        WindowShowState,
    };
    use crate::storage::LedgerStore;
    use crate::tiling_sys::IntentStorage;
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;

    const TAG: &str = "abcdef0123456789";

    fn owner() -> ProcessIdentity {
        ProcessIdentity {
            pid: 999,
            process_creation: "owner-creation".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\owner.exe".to_owned(),
        }
    }

    fn target_process() -> ProcessIdentity {
        ProcessIdentity {
            pid: 4242,
            process_creation: "proc-creation".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        }
    }

    fn open_store(tag: &str, owner: &ProcessIdentity, windows: Vec<WindowIdentity>) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omnitiler-na-mode-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = LedgerStore::open(&dir).expect("ledger open");
        store
            .commit(&RecoveryLedger {
                v: LEDGER_SCHEMA_VERSION,
                owner: owner.clone(),
                windows,
                mouse_snap: None,
            })
            .expect("seed ledger");
        dir
    }

    fn hide_claim(hwnd: u64, minimized: bool) -> WindowIdentity {
        WindowIdentity {
            hwnd,
            process: target_process(),
            tag: TAG.to_owned(),
            kind: WindowClaimKind::Product,
            show: WindowShowState {
                minimized,
                maximized: false,
            },
        }
    }

    fn register_window(hwnd: u64, visible: bool, iconic: bool, nonce: Option<String>) {
        fake::register(
            hwnd,
            FakeWindow {
                pid: 4242,
                process: target_process(),
                integrity_rid: 0x2000,
                nonce,
                member_tag: Some("member-tag-1".to_owned()),
                visible,
                iconic,
                maximized: false,
                alive: true,
                post_ok: true,
                defer: false,
            },
        );
    }

    #[test]
    fn pumped_hide_records_pumped_post() {
        let owner = owner();
        let dir = open_store("hide-pumped", &owner, Vec::new());
        let store = LedgerStore::open(&dir).expect("ledger reopen");
        register_window(9001, true, false, Some(TAG.to_owned()));
        let claim = hide_claim(9001, false);
        hide_managed_claim_with_mode(
            &store,
            &owner,
            &claim,
            &dir,
            VisibilityMode::PumpedShowAsync,
        )
        .expect("pumped hide");
        assert_eq!(fake::post_modes(), vec![(9001, true, true)]);
        fake::unregister_all();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nonactivating_flag_words_match_swp_expression() {
        // The actual `SetWindowPos` flag words in `post_async_pos` stay
        // pinned: hide `0x4097`, show `0x4057`. A flag change must update
        // the constants (and diagnostics vocabulary) with it.
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SWP_ASYNCWINDOWPOS, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SWP_NOZORDER, SWP_SHOWWINDOW,
        };
        let hide = SWP_NOMOVE
            | SWP_NOSIZE
            | SWP_NOZORDER
            | SWP_NOACTIVATE
            | SWP_ASYNCWINDOWPOS
            | SWP_HIDEWINDOW;
        let show = SWP_NOMOVE
            | SWP_NOSIZE
            | SWP_NOZORDER
            | SWP_NOACTIVATE
            | SWP_ASYNCWINDOWPOS
            | SWP_SHOWWINDOW;
        assert_eq!(hide, super::NA_POS_HIDE_FLAGS);
        assert_eq!(show, super::NA_POS_SHOW_FLAGS);
    }

    #[test]
    fn nonactivating_hide_records_unpumped_post() {
        let owner = owner();
        let dir = open_store("hide-na", &owner, Vec::new());
        let store = LedgerStore::open(&dir).expect("ledger reopen");
        register_window(9002, true, false, Some(TAG.to_owned()));
        let claim = hide_claim(9002, false);
        hide_managed_claim_with_mode(
            &store,
            &owner,
            &claim,
            &dir,
            VisibilityMode::NonactivatingPos,
        )
        .expect("na hide");
        assert_eq!(fake::post_modes(), vec![(9002, true, false)]);
        fake::unregister_all();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nonactivating_reveal_minimized_falls_back_to_pumped() {
        let owner = owner();
        let claim = hide_claim(9003, true);
        let dir = open_store("reveal-min", &owner, vec![claim.clone()]);
        let store = LedgerStore::open(&dir).expect("ledger reopen");
        register_window(9003, false, false, Some(TAG.to_owned()));
        reveal_product_claim_to_with_mode(
            &store,
            &owner,
            &claim,
            false,
            VisibilityMode::NonactivatingPos,
        )
        .expect("na minimized reveal");
        assert_eq!(fake::post_modes(), vec![(9003, false, true)]);
        fake::unregister_all();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nonactivating_reveal_plain_uses_unpumped_post() {
        let owner = owner();
        let claim = hide_claim(9004, false);
        let dir = open_store("reveal-plain", &owner, vec![claim.clone()]);
        let store = LedgerStore::open(&dir).expect("ledger reopen");
        register_window(9004, false, false, Some(TAG.to_owned()));
        reveal_product_claim_to_with_mode(
            &store,
            &owner,
            &claim,
            false,
            VisibilityMode::NonactivatingPos,
        )
        .expect("na plain reveal");
        assert_eq!(fake::post_modes(), vec![(9004, false, false)]);
        fake::unregister_all();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The traced pumped post records the actual `ShowWindowAsync` call:
    /// HWND, direction, command, verdict, and call/return stamps.
    #[test]
    fn traced_pumped_post_records_show_api_and_cmd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
        register_window(9101, true, false, Some(TAG.to_owned()));
        let mut sink = super::VisibilityPostTrace::default();
        assert!(super::post_async_traced(9101, SW_HIDE, Some(&mut sink)));
        assert_eq!(sink.rows.len(), 1);
        let row = &sink.rows[0];
        assert_eq!(row.hwnd, 9101);
        assert!(row.hide);
        assert_eq!(row.api, super::VIEW_POST_API_SHOW);
        assert_eq!(row.flags, None);
        assert_eq!(row.cmd, Some(SW_HIDE));
        assert!(row.posted);
        assert!(row.call_mono_ms <= row.ret_mono_ms);
        // Unarmed posts run the same native effect with no record.
        assert!(super::post_async_traced(9101, SW_HIDE, None));
        fake::unregister_all();
    }

    /// The traced nonactivating post records the actual `SetWindowPos`
    /// flag word, not the configured mode label.
    #[test]
    fn traced_pos_post_records_pos_api_and_flags() {
        register_window(9102, true, false, Some(TAG.to_owned()));
        let mut sink = super::VisibilityPostTrace::default();
        assert!(super::post_async_pos_traced(9102, true, Some(&mut sink)));
        assert_eq!(sink.rows.len(), 1);
        assert_eq!(sink.rows[0].api, super::VIEW_POST_API_POS);
        assert_eq!(sink.rows[0].flags, Some(super::NA_POS_HIDE_FLAGS));
        assert_eq!(sink.rows[0].cmd, None);
        assert!(sink.rows[0].hide && sink.rows[0].posted);
        assert!(super::post_async_pos_traced(9102, false, Some(&mut sink)));
        assert_eq!(sink.rows.len(), 2);
        assert_eq!(sink.rows[1].api, super::VIEW_POST_API_POS);
        assert_eq!(sink.rows[1].flags, Some(super::NA_POS_SHOW_FLAGS));
        assert!(!sink.rows[1].hide && sink.rows[1].posted);
        fake::unregister_all();
    }

    /// A refused native post still records the actual call with its
    /// `false` verdict: the record names what ran, never success.
    #[test]
    fn traced_refused_post_records_posted_false() {
        fake::register(
            9103,
            FakeWindow {
                pid: 4242,
                process: target_process(),
                integrity_rid: 0x2000,
                nonce: Some(TAG.to_owned()),
                member_tag: Some("member-tag-1".to_owned()),
                visible: true,
                iconic: false,
                maximized: false,
                alive: true,
                post_ok: false,
                defer: false,
            },
        );
        let mut sink = super::VisibilityPostTrace::default();
        assert!(!super::post_async_pos_traced(9103, true, Some(&mut sink)));
        assert_eq!(sink.rows.len(), 1);
        assert_eq!(sink.rows[0].api, super::VIEW_POST_API_POS);
        assert!(!sink.rows[0].posted);
        fake::unregister_all();
    }

    /// End-to-end traced hide/reveal records the executed API per post,
    /// including the minimized-reveal fallback to the pumped path.
    #[test]
    fn traced_hide_reveal_records_executed_api() {
        let owner = owner();
        let dir = open_store("trace-api", &owner, Vec::new());
        let store = LedgerStore::open(&dir).expect("ledger reopen");
        register_window(9104, true, false, Some(TAG.to_owned()));
        let claim = hide_claim(9104, false);
        let mut sink = super::VisibilityPostTrace::default();
        super::hide_managed_claim_with_mode_traced(
            &store,
            &owner,
            &claim,
            &dir,
            VisibilityMode::NonactivatingPos,
            Some(&mut sink),
        )
        .expect("traced na hide");
        assert_eq!(sink.rows.len(), 1);
        assert_eq!(sink.rows[0].api, super::VIEW_POST_API_POS);
        assert_eq!(sink.rows[0].flags, Some(super::NA_POS_HIDE_FLAGS));
        assert!(sink.rows[0].hide);
        // Plain reveal posts the nonactivating show.
        sink = super::VisibilityPostTrace::default();
        super::reveal_product_claim_to_with_mode_traced(
            &store,
            &owner,
            &claim,
            false,
            VisibilityMode::NonactivatingPos,
            Some(&mut sink),
        )
        .expect("traced na reveal");
        assert_eq!(sink.rows.len(), 1);
        assert_eq!(sink.rows[0].api, super::VIEW_POST_API_POS);
        assert_eq!(sink.rows[0].flags, Some(super::NA_POS_SHOW_FLAGS));
        assert!(!sink.rows[0].hide);
        fake::unregister_all();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A minimized restore keeps the pumped `SW_SHOWMINNOACTIVE` post even
    /// in the nonactivating mode, and the record says so.
    #[test]
    fn traced_minimized_reveal_records_pumped_fallback() {
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWMINNOACTIVE;
        let owner = owner();
        let claim = hide_claim(9105, true);
        let dir = open_store("trace-min", &owner, vec![claim.clone()]);
        let store = LedgerStore::open(&dir).expect("ledger reopen");
        register_window(9105, false, false, Some(TAG.to_owned()));
        let mut sink = super::VisibilityPostTrace::default();
        super::reveal_product_claim_to_with_mode_traced(
            &store,
            &owner,
            &claim,
            false,
            VisibilityMode::NonactivatingPos,
            Some(&mut sink),
        )
        .expect("traced na minimized reveal");
        assert_eq!(sink.rows.len(), 1);
        assert_eq!(sink.rows[0].api, super::VIEW_POST_API_SHOW);
        assert_eq!(sink.rows[0].cmd, Some(SW_SHOWMINNOACTIVE));
        fake::unregister_all();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Early vetos (here: bad claim shape) record nothing: the sink never
    /// infers a call from the configured mode.
    #[test]
    fn traced_hide_shape_refusal_records_nothing() {
        let owner = owner();
        let dir = open_store("trace-veto", &owner, Vec::new());
        let store = LedgerStore::open(&dir).expect("ledger reopen");
        let mut bad = hide_claim(9106, false);
        bad.tag = "bad-tag".to_owned();
        let mut sink = super::VisibilityPostTrace::default();
        let result = super::hide_managed_claim_with_mode_traced(
            &store,
            &owner,
            &bad,
            &dir,
            VisibilityMode::NonactivatingPos,
            Some(&mut sink),
        );
        assert!(result.is_err());
        assert!(sink.rows.is_empty() && sink.total == 0);
        fake::unregister_all();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The source accumulator caps storage with an honest total: past the
    /// row cap every executed post still counts, so the world-apply
    /// omission derives from the total instead of a false zero.
    #[test]
    fn traced_source_storage_caps_with_honest_total() {
        use super::{MAX_VISIBILITY_POSTS, VisibilityPostDiag};
        let mut sink = super::VisibilityPostTrace::default();
        for n in 0..(MAX_VISIBILITY_POSTS + 3) {
            sink.push(VisibilityPostDiag {
                hwnd: 9200 + n as u64,
                hide: true,
                api: super::VIEW_POST_API_POS,
                flags: Some(super::NA_POS_HIDE_FLAGS),
                cmd: None,
                posted: true,
                call_mono_ms: 11,
                ret_mono_ms: 13,
            });
        }
        assert_eq!(sink.rows.len(), MAX_VISIBILITY_POSTS);
        assert_eq!(sink.total, MAX_VISIBILITY_POSTS + 3);
        let (rows, total) = sink.take();
        assert_eq!(rows.len(), MAX_VISIBILITY_POSTS);
        assert_eq!(total, MAX_VISIBILITY_POSTS + 3);
        assert_eq!(rows[0].hwnd, 9200);
    }

    // ---- R12 Task 8: one packed lifetime-bound intent slot ----

    const SLOT_PID: u32 = 4242;
    const SLOT_CREATION: &str = "create-slot";

    fn slot_storage(sticky: Option<bool>, float: bool, tile: bool) -> IntentStorage {
        IntentStorage {
            sticky_prior: sticky,
            float,
            tile,
        }
    }

    /// Scripted slot table stand-in for the native window property: `None`
    /// is an absent property, `Some(stored)` is present. Decoding against
    /// the fixed test lifetime mirrors [`super::read_intent_slot`].
    fn read_scripted(table: &RefCell<Option<usize>>) -> Option<Option<IntentStorage>> {
        match *table.borrow() {
            None => Some(None),
            Some(stored) => decode_intent_slot(stored, SLOT_PID, SLOT_CREATION, TAG).map(Some),
        }
    }

    #[test]
    fn intent_slot_round_trips_all_flag_combinations() {
        let mut distinct = Vec::new();
        for sticky in [None, Some(false), Some(true)] {
            for float in [false, true] {
                for tile in [false, true] {
                    let storage = slot_storage(sticky, float, tile);
                    let stored = encode_intent_slot(&storage, SLOT_PID, SLOT_CREATION, TAG)
                        .expect("every valid storage encodes");
                    assert_ne!(stored, 0, "a stored zero is never a valid slot");
                    assert_eq!(
                        decode_intent_slot(stored, SLOT_PID, SLOT_CREATION, TAG),
                        Some(storage),
                        "round trip for {storage:?}"
                    );
                    distinct.push(stored);
                }
            }
        }
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 12, "all 12 combinations encode distinctly");
    }

    #[test]
    fn intent_slot_foreign_pid_or_lifetime_reads_unreadable() {
        let storage = slot_storage(Some(true), true, true);
        let stored =
            encode_intent_slot(&storage, SLOT_PID, SLOT_CREATION, TAG).expect("encodes");
        assert_eq!(
            decode_intent_slot(stored, SLOT_PID + 1, SLOT_CREATION, TAG),
            None,
            "wrong pid is unreadable"
        );
        assert_eq!(
            decode_intent_slot(stored, SLOT_PID, "other-creation", TAG),
            None,
            "wrong process creation is unreadable"
        );
        assert_eq!(
            decode_intent_slot(stored, SLOT_PID, SLOT_CREATION, "fedcba9876543210"),
            None,
            "wrong member tag is unreadable"
        );
        assert_ne!(
            decode_intent_slot(stored, SLOT_PID + 1, SLOT_CREATION, TAG),
            Some(IntentStorage::default()),
            "a foreign lifetime never reads as canonical no-intent"
        );
    }

    #[test]
    fn intent_slot_corrupt_code_or_zero_reads_unreadable() {
        assert_eq!(
            decode_intent_slot(0, SLOT_PID, SLOT_CREATION, TAG),
            None,
            "the never-stored zero value is corrupt"
        );
        let storage = slot_storage(Some(false), true, false);
        let stored = encode_intent_slot(&storage, SLOT_PID, SLOT_CREATION, TAG).expect("encodes");
        let encoded = u64::try_from(stored).expect("64-bit") - 1;

        let code3 = usize::try_from((encoded & !0b11) | 0b11).expect("fits");
        assert_eq!(
            decode_intent_slot(code3 + 1, SLOT_PID, SLOT_CREATION, TAG),
            None,
            "sticky code 3 is invalid"
        );

        let reserved = usize::try_from(encoded | 0x10).expect("fits");
        assert_eq!(
            decode_intent_slot(reserved + 1, SLOT_PID, SLOT_CREATION, TAG),
            None,
            "a stray reserved bit is corrupt"
        );

        let foreign_hash = usize::try_from(encoded ^ (1 << 40)).expect("fits");
        assert_eq!(
            decode_intent_slot(foreign_hash + 1, SLOT_PID, SLOT_CREATION, TAG),
            None,
            "a foreign creation/tag hash is unreadable"
        );
    }

    #[test]
    fn intent_slot_transition_rewrites_all_bits_in_one_set() {
        let table = RefCell::new(None);
        let sets = Cell::new(0u32);
        let removes = Cell::new(0u32);
        let commit = |desired: IntentStorage| {
            commit_intent_slot(
                desired,
                (SLOT_PID, SLOT_CREATION, TAG),
                |stored| {
                    sets.set(sets.get() + 1);
                    *table.borrow_mut() = Some(stored);
                    true
                },
                || {
                    removes.set(removes.get() + 1);
                    *table.borrow_mut() = None;
                },
                || true,
                || read_scripted(&table),
            )
        };

        commit(slot_storage(Some(false), false, false)).expect("install sticky");
        let before = u64::try_from(table.borrow().expect("slot present")).expect("64-bit") - 1;
        sets.set(0);

        commit(slot_storage(None, true, true)).expect("one rewrite to float+tile");
        assert_eq!(sets.get(), 1, "exactly one native set per transition");
        assert_eq!(removes.get(), 0, "no remove on a non-empty transition");
        let after = u64::try_from(table.borrow().expect("slot present")).expect("64-bit") - 1;
        assert_ne!(
            before & 0b11,
            after & 0b11,
            "sticky code changed in the same write"
        );
        assert_eq!(after & INTENT_FLOAT_BIT, INTENT_FLOAT_BIT);
        assert_eq!(after & INTENT_TILE_BIT, INTENT_TILE_BIT);
        assert_eq!(
            read_scripted(&table),
            Some(Some(slot_storage(None, true, true)))
        );
    }

    #[test]
    fn intent_slot_failed_set_keeps_previous_storage() {
        let previous = slot_storage(Some(true), true, false);
        let stored =
            encode_intent_slot(&previous, SLOT_PID, SLOT_CREATION, TAG).expect("encodes");
        let table = RefCell::new(Some(stored));
        let set_calls = Cell::new(0u32);
        let result = commit_intent_slot(
            slot_storage(None, false, true),
            (SLOT_PID, SLOT_CREATION, TAG),
            |_| {
                set_calls.set(set_calls.get() + 1);
                false
            },
            || panic!("the empty branch must not run"),
            || panic!("legacy retirement must not run on a failed set"),
            || read_scripted(&table),
        );
        assert_eq!(result, Err(()), "a refused SetProp fails the transition");
        assert_eq!(set_calls.get(), 1, "exactly one attempted set");
        assert_eq!(
            read_scripted(&table),
            Some(Some(previous)),
            "the previous storage stays readable and unchanged"
        );
    }

    #[test]
    fn intent_slot_empty_transition_retires_legacy_before_removal() {
        let stored = encode_intent_slot(
            &slot_storage(None, true, false),
            SLOT_PID,
            SLOT_CREATION,
            TAG,
        )
        .expect("encodes");
        let table = RefCell::new(Some(stored));
        let order = RefCell::new(Vec::new());
        let result = commit_intent_slot(
            IntentStorage::default(),
            (SLOT_PID, SLOT_CREATION, TAG),
            |_| panic!("an empty transition never sets"),
            || {
                order.borrow_mut().push("remove");
                *table.borrow_mut() = None;
            },
            || {
                order.borrow_mut().push("retire");
                true
            },
            || read_scripted(&table),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            &*order.borrow(),
            &["retire", "remove"],
            "legacy residue retires before the slot removal commits"
        );
        assert_eq!(read_scripted(&table), Some(None));
    }

    #[test]
    fn intent_slot_empty_transition_aborts_when_legacy_retire_fails() {
        let previous = slot_storage(None, true, false);
        let stored =
            encode_intent_slot(&previous, SLOT_PID, SLOT_CREATION, TAG).expect("encodes");
        let table = RefCell::new(Some(stored));
        let remove_calls = Cell::new(0u32);
        let result = commit_intent_slot(
            IntentStorage::default(),
            (SLOT_PID, SLOT_CREATION, TAG),
            |_| panic!("an empty transition never sets"),
            || {
                remove_calls.set(remove_calls.get() + 1);
                *table.borrow_mut() = None;
            },
            || false,
            || read_scripted(&table),
        );
        assert_eq!(result, Err(()));
        assert_eq!(
            remove_calls.get(),
            0,
            "the slot stays until legacy residue is verifiably gone"
        );
        assert_eq!(read_scripted(&table), Some(Some(previous)));
    }

    #[test]
    fn intent_change_preserves_other_lanes() {
        let base = slot_storage(Some(false), true, true);
        assert_eq!(
            apply_intent_change(base, IntentChange::SetSticky(true)),
            slot_storage(Some(true), true, true)
        );
        assert_eq!(
            apply_intent_change(base, IntentChange::ClearSticky),
            slot_storage(None, true, true)
        );
        assert_eq!(
            apply_intent_change(slot_storage(None, true, false), IntentChange::SetFloat),
            slot_storage(None, true, false)
        );
        assert_eq!(
            apply_intent_change(base, IntentChange::ClearFloat),
            slot_storage(Some(false), false, true)
        );
        assert_eq!(
            apply_intent_change(slot_storage(None, false, false), IntentChange::SetTile),
            slot_storage(None, false, true)
        );
        assert_eq!(
            apply_intent_change(base, IntentChange::ClearTile),
            slot_storage(Some(false), true, false)
        );
    }

    #[test]
    fn sticky_remove_verdict_never_touches_a_foreign_prior() {
        assert_eq!(
            sticky_clear_verdict(&slot_storage(None, false, false), true),
            StickyClearVerdict::AlreadyAbsent
        );
        assert_eq!(
            sticky_clear_verdict(&slot_storage(Some(false), false, false), true),
            StickyClearVerdict::NotOwned
        );
        assert_eq!(
            sticky_clear_verdict(&slot_storage(Some(true), false, false), false),
            StickyClearVerdict::NotOwned
        );
        assert_eq!(
            sticky_clear_verdict(&slot_storage(Some(false), false, false), false),
            StickyClearVerdict::Clear
        );
    }

    #[test]
    fn helper_transitions_over_packed_slot_keep_documented_semantics() {
        let table = RefCell::new(None);
        let transition = |change: IntentChange| -> Result<IntentStorage, ()> {
            let current = match read_scripted(&table) {
                None => return Err(()),
                Some(None) => IntentStorage::default(),
                Some(Some(storage)) => storage,
            };
            let desired = apply_intent_change(current, change);
            commit_intent_slot(
                desired,
                (SLOT_PID, SLOT_CREATION, TAG),
                |stored| {
                    *table.borrow_mut() = Some(stored);
                    true
                },
                || *table.borrow_mut() = None,
                || true,
                || read_scripted(&table),
            )?;
            Ok(desired)
        };

        // Install float, then sticky(true) keeps the float bit.
        assert_eq!(
            transition(IntentChange::SetFloat),
            Ok(slot_storage(None, true, false))
        );
        assert_eq!(
            transition(IntentChange::SetSticky(true)),
            Ok(slot_storage(Some(true), true, false))
        );
        // A different live prior refuses without touching the slot.
        let live = match read_scripted(&table) {
            Some(Some(storage)) => storage,
            other => panic!("live storage expected, got {other:?}"),
        };
        assert_eq!(
            sticky_clear_verdict(&live, false),
            StickyClearVerdict::NotOwned
        );
        assert_eq!(
            read_scripted(&table),
            Some(Some(slot_storage(Some(true), true, false)))
        );
        // The exact live prior clears sticky and keeps float.
        assert_eq!(
            transition(IntentChange::ClearSticky),
            Ok(slot_storage(None, true, false))
        );
        // Clearing the last bit empties the storage and removes the slot.
        assert_eq!(
            transition(IntentChange::ClearFloat),
            Ok(IntentStorage::default())
        );
        assert_eq!(read_scripted(&table), Some(None));
    }
}
