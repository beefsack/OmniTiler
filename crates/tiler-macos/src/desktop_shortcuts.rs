#![forbid(unsafe_code)]

//! Authentic Desktop-N shortcut ownership with explicit Apply/Revert.
//!
//! Approved route (user 2026-10-16): numbered sends use "Switch to Desktop
//! N" chords that Authentic enables with owned preimages and Revert. The tap
//! passes the tiler's own synthetic switch chords while still consuming the
//! user's identical chords (see [`crate::input_match`]).
//!
//! Live mechanism (tier 2, no security change): the private SkyLight SPI
//! used by Silica (`CGSGetSymbolicHotKeyValue` / `CGSIsSymbolicHotKeyEnabled`
//! / `CGSSetSymbolicHotKeyEnabled`, see [`crate::ffi`] for the pinned
//! widths) enables holders session-effectively. **No plist is ever
//! written** by this module: the preimage covers exactly the
//! SPI-observable session subset (enabled plus the posting chord), never
//! the full persistent `com.apple.symbolichotkeys` dictionary (which may
//! carry more fields — nothing outside the observed subset is at risk
//! because only the enabled flag is ever written, and the chord is never
//! modified). Logout/reboot persistence of the SPI enable is unverified
//! and never claimed ([`SESSION_PERSISTENCE_NOTE`]).
//!
//! Backup discipline: the durable preimage publishes BEFORE the first
//! settings write and is never overwritten — a second Apply resumes from
//! the stored first-observed preimages (already-applied IDs are verified,
//! never rewritten; external edits are preserved, never overwritten) and
//! only appends records for IDs the first run never observed, atomically.
//! A publish refusal writes nothing. Revert restores an entry only when
//! the current value still equals what Apply wrote; a later external edit
//! is preserved and reported. Startup and tile paths never apply: only
//! the explicit commands do.
//!
//! Coverage is capped at Desktops 1-10 ([`APPLY_MAX_COUNT`]): the only IDs
//! with default digit chords. Higher IDs have no demonstrated need.
//!
//! Prior art (read-only checkouts): Silica `SISystemWideElement.m`
//! `eventForSwitchingToSpace:` derives `118 + space - 1`, resolves the
//! chord, and enables the holder before posting; that unit wraps the same
//! three calls in explicit Apply with preimages plus Revert.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// First "Switch to Desktop N" symbolic ID: Desktop 1 (Silica parity:
/// `(unsigned short)(118 + space - 1)`).
pub const DESKTOP_SYMBOLIC_BASE: u32 = 118;
/// Number of Desktop-N holders (IDs 118-133, Desktops 1-16).
pub const DESKTOP_MAX: u32 = 16;
/// Apply coverage cap: Desktops 1-10, the only IDs with default digit
/// chords. Higher IDs have no demonstrated need and stay refused.
pub const APPLY_MAX_COUNT: u32 = 10;
/// Default Apply coverage: Desktops 1-9, matching numbered select/send
/// (`Meta+1..9` / `Meta+Shift+1..9`).
pub const DEFAULT_APPLY_COUNT: u32 = 9;
/// Durable preimage file schema version.
pub const PREIMAGE_SCHEMA_VERSION: u32 = 1;
/// Honest reboot semantics, carried in every Apply/Revert report: the
/// enable is session-effective only, no plist is written, startup never
/// applies, and logout/reboot persistence is unverified.
pub const SESSION_PERSISTENCE_NOTE: &str = "session-effective SkyLight SPI enable only: no plist is written, startup never applies, and logout/reboot persistence is unverified — Revert restores within-session state; reboot behavior is TBD, never claimed persistent";

/// `CGEventFlags` Control bit (event-flags domain, as `CGSGetSymbolicHotKeyValue`
/// reports and Silica posts): the modifier leg of every default Desktop-N chord.
/// Pinned equal to [`crate::input_match::SWITCH_CTRL_BIT`] by test.
pub const CG_EVENT_CTRL_BIT: u64 = 0x0004_0000;

/// Mac virtual keycodes for the Desktop-N digit legs (`CGKeyCode` space,
/// ANSI positions: `kVK_ANSI_*` in `Events.h`). Desktop 10 uses the `0` key.
pub const KEY_1: u32 = 0x12;
pub const KEY_2: u32 = 0x13;
pub const KEY_3: u32 = 0x14;
pub const KEY_4: u32 = 0x15;
pub const KEY_5: u32 = 0x17;
pub const KEY_6: u32 = 0x16;
pub const KEY_7: u32 = 0x1A;
pub const KEY_8: u32 = 0x1C;
pub const KEY_9: u32 = 0x19;
pub const KEY_0: u32 = 0x1D;

/// Symbolic ID for Desktop N (1-based, `118+N-1`). `None` outside 1-16.
#[must_use]
pub const fn desktop_symbolic_id(desktop: u32) -> Option<u32> {
    if desktop >= 1 && desktop <= DESKTOP_MAX {
        Some(DESKTOP_SYMBOLIC_BASE + desktop - 1)
    } else {
        None
    }
}

/// Desktop N for a symbolic ID. `None` outside 118-133.
#[must_use]
pub const fn symbolic_id_desktop(symbolic_id: u32) -> Option<u32> {
    if symbolic_id >= DESKTOP_SYMBOLIC_BASE && symbolic_id < DESKTOP_SYMBOLIC_BASE + DESKTOP_MAX {
        Some(symbolic_id - DESKTOP_SYMBOLIC_BASE + 1)
    } else {
        None
    }
}

/// Digit keycode for Desktop N (1-9 map to `1`-`9`, 10 maps to `0`).
/// Desktops 11-16 have no default digit chord: `None` (unresolved, never
/// guessed).
#[must_use]
pub const fn desktop_digit_keycode(desktop: u32) -> Option<u32> {
    match desktop {
        1 => Some(KEY_1),
        2 => Some(KEY_2),
        3 => Some(KEY_3),
        4 => Some(KEY_4),
        5 => Some(KEY_5),
        6 => Some(KEY_6),
        7 => Some(KEY_7),
        8 => Some(KEY_8),
        9 => Some(KEY_9),
        10 => Some(KEY_0),
        _ => None,
    }
}

/// True for the ten digit keycodes owned by Desktops 1-10.
#[must_use]
pub const fn is_digit_keycode(keycode: u32) -> bool {
    matches!(
        keycode,
        KEY_1 | KEY_2 | KEY_3 | KEY_4 | KEY_5 | KEY_6 | KEY_7 | KEY_8 | KEY_9 | KEY_0
    )
}

/// One Desktop-N holder's SPI-observable session state: the posting chord
/// (virtual keycode plus raw modifier flags in event-flags domain, exactly
/// as `CGSGetSymbolicHotKeyValue` reports and Silica posts) plus its
/// enabled flag. This is the session subset only — never the full
/// persistent plist dictionary, which may carry more fields. Only `enabled`
/// is ever written, so nothing outside this subset is at risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutEntry {
    pub enabled: bool,
    pub keycode: u32,
    pub modifiers: u64,
}

impl ShortcutEntry {
    /// The entry Apply enforces: the observed chord, enabled.
    #[must_use]
    pub const fn applied(self) -> Self {
        Self {
            enabled: true,
            keycode: self.keycode,
            modifiers: self.modifiers,
        }
    }
}

/// Per-ID session override store: reads observe exactly one holder;
/// `set_enabled` flips only the enabled flag (the chord is never
/// written, no plist touched). `Err` is unresolvable (missing SPI,
/// invalid ID, unreadable, injected failure) — never guessed.
pub trait DesktopShortcutStore {
    fn read(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String>;
    fn set_enabled(&mut self, symbolic_id: u32, enabled: bool) -> Result<(), String>;
}

/// Fake session store: exact preimages plus injectable per-ID failures and
/// call counters (the counters prove resume paths never rewrite). No live
/// touch.
#[derive(Debug, Default)]
pub struct FakeStore {
    pub(crate) entries: HashMap<u32, ShortcutEntry>,
    /// Reads on these IDs fail (covers preimage and readback reads).
    pub fail_reads: HashSet<u32>,
    /// Enable flips on these IDs fail.
    pub fail_sets: HashSet<u32>,
    /// Observed calls, for no-redundant-write proofs.
    pub reads: u64,
    pub writes: u64,
}

impl FakeStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_entry(mut self, symbolic_id: u32, entry: ShortcutEntry) -> Self {
        self.entries.insert(symbolic_id, entry);
        self
    }

    #[must_use]
    pub fn get(&self, symbolic_id: u32) -> Option<ShortcutEntry> {
        self.entries.get(&symbolic_id).copied()
    }
}

impl DesktopShortcutStore for FakeStore {
    fn read(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        self.reads += 1;
        if self.fail_reads.contains(&symbolic_id) {
            return Err(format!("fake read refused for id {symbolic_id}"));
        }
        self.entries.get(&symbolic_id).copied().ok_or_else(|| {
            format!("fake id {symbolic_id} unresolvable; refusing rather than guessing")
        })
    }

    fn set_enabled(&mut self, symbolic_id: u32, enabled: bool) -> Result<(), String> {
        self.writes += 1;
        if self.fail_sets.contains(&symbolic_id) {
            return Err(format!("fake enable refused for id {symbolic_id}"));
        }
        let entry = self.entries.get_mut(&symbolic_id).ok_or_else(|| {
            format!("fake id {symbolic_id} unresolvable; refusing rather than guessing")
        })?;
        entry.enabled = enabled;
        Ok(())
    }
}

/// Narrow SPI-unavailable refusal: no preference was read or written.
const SPI_UNAVAILABLE: &str = "session hotkey SPI unavailable (SkyLight CGSGet/SetSymbolicHotKeyValue or CGSIsSymbolicHotKeyEnabled missing); no preference was read or written";

/// Live session store over [`crate::ffi`] SPI. Every op is a real
/// session read/write or a narrow refusal for genuinely missing SPI.
pub struct LiveStore {
    spi: Option<crate::ffi::HotkeySpi>,
}

impl LiveStore {
    /// Resolve once (read-only symbol lookup). Production entry point.
    #[must_use]
    pub fn new() -> Self {
        Self {
            spi: crate::ffi::resolve_hotkey_spi(),
        }
    }

    /// Refusal-path seam: never touches the OS. Tests prove the
    /// genuinely-missing-SPI refusal through this; production uses `new`.
    #[must_use]
    pub fn without_spi() -> Self {
        Self { spi: None }
    }
    fn spi(&self) -> Result<crate::ffi::HotkeySpi, String> {
        self.spi.ok_or_else(|| SPI_UNAVAILABLE.to_owned())
    }
}

impl Default for LiveStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopShortcutStore for LiveStore {
    fn read(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        let spi = self.spi()?;
        let (keycode, modifiers, enabled) = crate::ffi::hotkey_read(spi, symbolic_id)?;
        Ok(ShortcutEntry {
            enabled,
            keycode,
            modifiers,
        })
    }

    fn set_enabled(&mut self, symbolic_id: u32, enabled: bool) -> Result<(), String> {
        let spi = self.spi()?;
        crate::ffi::hotkey_set_enabled(spi, symbolic_id, enabled)
    }
}

/// One owned preimage record: the fully SPI-observed session state before
/// Apply plus what Apply wrote and read back (the same chord, enabled).
/// Unresolvable IDs never reach storage: planning refuses them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopPreimageRecord {
    pub symbolic_id: u32,
    pub desktop: u32,
    pub preimage: ShortcutEntry,
    pub applied: ShortcutEntry,
}

/// Durable preimage file: published BEFORE the first settings write,
/// never overwritten (resume appends only unobserved IDs), removed only
/// on a clean full Revert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopPreimageFile {
    pub version: u32,
    pub desktops: Vec<DesktopPreimageRecord>,
}

/// Pure preimage validation over plain data: schema version, non-empty,
/// unique in-range IDs each matching its desktop, sane keycodes, applied
/// entries enabled with exactly the preimage chord (enable-only
/// invariant). Hostile files refuse before any store touch.
pub fn validate_preimage_file(file: &DesktopPreimageFile) -> Result<(), String> {
    if file.version != PREIMAGE_SCHEMA_VERSION {
        return Err(format!(
            "desktop preimage version {} unsupported (expected {PREIMAGE_SCHEMA_VERSION})",
            file.version
        ));
    }
    if file.desktops.is_empty() {
        return Err("desktop preimage has no records; refusing".to_owned());
    }
    let mut seen = HashSet::new();
    for record in &file.desktops {
        if !seen.insert(record.symbolic_id) {
            return Err(format!(
                "desktop preimage duplicates id {}; refusing",
                record.symbolic_id
            ));
        }
        let Some(desktop) = symbolic_id_desktop(record.symbolic_id) else {
            return Err(format!(
                "desktop preimage id {} out of range 118-133; refusing",
                record.symbolic_id
            ));
        };
        if record.desktop != desktop {
            return Err(format!(
                "desktop preimage id {} names desktop {} (expected {desktop}); refusing",
                record.symbolic_id, record.desktop
            ));
        }
        validate_entry(&record.preimage, record.symbolic_id)?;
        validate_entry(&record.applied, record.symbolic_id)?;
        if !record.applied.enabled {
            return Err(format!(
                "desktop preimage id {} applied entry is disabled; refusing",
                record.symbolic_id
            ));
        }
        if record.applied.keycode != record.preimage.keycode
            || record.applied.modifiers != record.preimage.modifiers
        {
            return Err(format!(
                "desktop preimage id {} applied chord differs from preimage chord; refusing (enable-only invariant)",
                record.symbolic_id
            ));
        }
    }
    Ok(())
}

fn validate_entry(entry: &ShortcutEntry, symbolic_id: u32) -> Result<(), String> {
    if entry.keycode > 255 {
        return Err(format!(
            "desktop preimage id {symbolic_id} keycode {} out of range 0..=255; refusing",
            entry.keycode
        ));
    }
    Ok(())
}

/// Pure restore decision over plain data. `applied` is what Apply wrote
/// and read back, `current` is the fresh read:
/// - current already equals the preimage: nothing to do (no write);
/// - current still equals applied: restore the preimage enabled flag;
/// - anything else: a later external edit — preserve it, never overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAction {
    AlreadyRestored,
    SetEnabled(bool),
    KeepExternal,
}

#[must_use]
pub const fn decide_restore(
    preimage: ShortcutEntry,
    applied: ShortcutEntry,
    current: ShortcutEntry,
) -> RestoreAction {
    if current.enabled == preimage.enabled
        && current.keycode == preimage.keycode
        && current.modifiers == preimage.modifiers
    {
        return RestoreAction::AlreadyRestored;
    }
    if current.enabled == applied.enabled
        && current.keycode == applied.keycode
        && current.modifiers == applied.modifiers
    {
        return RestoreAction::SetEnabled(preimage.enabled);
    }
    RestoreAction::KeepExternal
}

/// Default expected session chord for Desktop N: the exact `Ctrl+digit`
/// shape a synthetic transfer post carries (digit keycode plus
/// [`CG_EVENT_CTRL_BIT`]). A stored preimage with any other chord is
/// preserved verbatim by Apply/Revert but flagged unusable for the
/// default transfer: the product transfer must refuse that mismatch
/// later rather than post a chord that does nothing. `None` for Desktops
/// 11-16 (no default digit chord; unreachable under the Apply cap).
#[must_use]
pub fn default_expected_chord(desktop: u32) -> Option<ShortcutEntry> {
    desktop_digit_keycode(desktop).map(|keycode| ShortcutEntry {
        enabled: true,
        keycode,
        modifiers: CG_EVENT_CTRL_BIT,
    })
}

/// Pure transfer-usability verdict over plain data: the observed chord is
/// usable for the default synthetic transfer only when it exactly equals
/// the desktop's default expected chord. No live touch.
#[must_use]
pub fn transfer_usable(desktop: u32, observed: ShortcutEntry) -> bool {
    default_expected_chord(desktop).is_some_and(|expected| {
        observed.keycode == expected.keycode && observed.modifiers == expected.modifiers
    })
}

/// Recovery backup file name under the per-user application-support
/// directory (see [`canonical_preimage_path_for`]).
pub const RECOVERY_FILE_NAME: &str = "desktop-shortcuts-preimage.json";

/// Durable recovery directory for one home directory (`None` without a
/// home): the per-user application-support subdirectory, derived through
/// [`crate::dev_selection::settings_path_for`] (the parent of
/// `settings.json`) so the settings and recovery conventions can never
/// drift apart. Never a temporary directory: session-effectively enabled
/// holders deserve a recovery record that survives logout/reboot even
/// though the enable itself is session-only and unverified across it.
#[must_use]
pub fn recovery_dir_for(home: Option<&str>) -> Option<PathBuf> {
    crate::dev_selection::settings_path_for(home)
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
}

/// Runtime durable recovery directory from `$HOME` (`None` when unset —
/// callers refuse; there is no temporary fallback).
#[must_use]
pub fn recovery_dir() -> Option<PathBuf> {
    recovery_dir_for(std::env::var("HOME").ok().as_deref())
}

/// Canonical durable preimage path for one home directory (`None` without
/// a home). The private owned recovery dir, never the temporary probe
/// session dir.
#[must_use]
pub fn canonical_preimage_path_for(home: Option<&str>) -> Option<PathBuf> {
    recovery_dir_for(home).map(|dir| dir.join(RECOVERY_FILE_NAME))
}

/// Runtime canonical durable preimage path from `$HOME` (`None` when
/// unset — callers refuse; there is no temporary fallback).
#[must_use]
pub fn canonical_preimage_path() -> Option<PathBuf> {
    canonical_preimage_path_for(std::env::var("HOME").ok().as_deref())
}

/// Ensure the private owned recovery dir: never a symlink, mode `0700`,
/// owned by the current UID. Fails closed on any deviation. Pure over an
/// explicit path so tests prove the hardening on fake dirs; production
/// resolves the path through [`recovery_dir`] (no `$HOME` means no path,
/// never a temporary fallback).
pub(crate) fn ensure_private_dir(dir: &std::path::Path) -> Result<(), String> {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err("desktop recovery dir is a symlink; refusing".to_owned());
            }
            if !meta.is_dir() {
                return Err("desktop recovery dir is not a directory; refusing".to_owned());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dir).map_err(|e| format!("desktop recovery dir: {e}"))?;
        }
        Err(e) => return Err(format!("desktop recovery dir metadata: {e}")),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("desktop recovery dir permissions: {e}"))?;
        let meta =
            std::fs::metadata(dir).map_err(|e| format!("desktop recovery dir metadata: {e}"))?;
        if meta.mode() & 0o777 != 0o700 {
            return Err("desktop recovery dir mode is not 0700; refusing".to_owned());
        }
        if meta.uid() != crate::ffi::current_uid() {
            return Err("desktop recovery dir not owned by current UID; refusing".to_owned());
        }
        if std::fs::symlink_metadata(dir)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            return Err("desktop recovery dir became a symlink; refusing".to_owned());
        }
    }
    Ok(())
}

/// Ensure the runtime durable recovery dir, creating it private when
/// absent. No `$HOME` refuses outright (no temporary fallback).
pub fn ensure_private_recovery_dir() -> Result<PathBuf, String> {
    let Some(dir) = recovery_dir() else {
        return Err("desktop recovery: no $HOME; refusing (no temporary fallback)".to_owned());
    };
    ensure_private_dir(&dir)?;
    Ok(dir)
}

/// Persist a preimage path's parent directory entry (Unix `fsync` on the
/// open dir): after a fresh hard-link or a merged rename the file content
/// is already synced, but the name itself is durable only once the parent
/// is. Callers treat a refusal as a publish failure (zero settings writes
/// after it). No-op off Unix (durability primitive unavailable there; all
/// real targets are Unix).
pub(crate) fn sync_parent_dir(path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let Some(parent) = path.parent() else {
            return Err("desktop preimage has no parent; refusing".to_owned());
        };
        std::fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| format!("desktop preimage parent sync: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Private lock-file path for a preimage path: the sibling leaf with a
/// `lock` extension. The lock serializes full Apply/Revert transactions
/// (read/merge/write/remove) across processes.
#[must_use]
pub fn lock_path_for(preimage_path: &std::path::Path) -> PathBuf {
    preimage_path.with_extension("lock")
}

/// Publish the preimage file BEFORE the first settings write. An existing
/// file (even valid) refuses: the original owned backup is never
/// overwritten — resume paths load it instead. Refuses on a symlink at
/// the path; publishes atomically with no-clobber semantics (temp file
/// plus `hard_link`, which fails when the destination exists — the
/// pre-check plus link is still two steps, so callers hold the
/// transaction lock across check and publish). Temp files carry a PID
/// nonce so concurrent writers never share a temp name; stale temps are
/// removed before writing. Mode `0600` on Unix.
pub fn publish_preimage_file_to(
    path: &std::path::Path,
    file: &DesktopPreimageFile,
) -> Result<(), String> {
    validate_preimage_file(file)?;
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err("desktop preimage path is a symlink; refusing".to_owned());
            }
            return Err(
                "desktop preimage already exists; refusing overwrite (resume from it or revert first)"
                    .to_owned(),
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("desktop preimage metadata: {e}")),
    }
    let json = serde_json::to_string(file).map_err(|e| format!("desktop preimage encode: {e}"))?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let write_tmp = || -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|e| format!("desktop preimage write: {e}"))?;
            f.write_all(json.as_bytes())
                .map_err(|e| format!("desktop preimage write: {e}"))?;
            f.sync_all()
                .map_err(|e| format!("desktop preimage write: {e}"))?;
            drop(f);
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("desktop preimage permissions: {e}"))?;
        }
        #[cfg(not(unix))]
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|e| format!("desktop preimage write: {e}"))?;
            f.write_all(json.as_bytes())
                .map_err(|e| format!("desktop preimage write: {e}"))?;
        }
        Ok(())
    };
    write_tmp()?;
    // Atomic no-clobber link: fails when the destination appeared after
    // the pre-check (a second publisher), leaving the first backup intact.
    // The transaction lock serializes publishers across processes (see
    // `acquire_transaction_lock`), so the link race below is a leftover
    // guard, never the plan.
    match std::fs::hard_link(&tmp, path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&tmp);
            // Persist the directory entry before any settings write: a
            // sync refusal fails the publish (zero settings writes
            // after it) while the synced-content backup is retained for
            // a resume retry, which replays it verbatim.
            sync_parent_dir(path)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                return Err(
                    "desktop preimage already exists; refusing overwrite (resume from it or revert first)"
                        .to_owned(),
                );
            }
            Err(format!("desktop preimage publish: {e}"))
        }
    }
}

/// Atomically publish an already-validated merged file (resume path):
/// stored records carried identically, new observations appended. The
/// merge validates before publish; no exists-guard here — the caller
/// loaded the file this run while holding the transaction lock (see
/// `acquire_transaction_lock`), which serializes full Apply/Revert
/// sequences across processes, so the loaded file is current within this
/// lock and the merge only appends IDs it never observed. The parent dir
/// syncs after the rename before any settings write, like the fresh
/// publish; a sync refusal fails the run with zero further writes (the
/// merged file itself is already replaced, validated, and resume-safe).
fn publish_merged_file_to(
    path: &std::path::Path,
    merged: &DesktopPreimageFile,
) -> Result<(), String> {
    validate_preimage_file(merged)?;
    let json =
        serde_json::to_string(&merged).map_err(|e| format!("desktop preimage encode: {e}"))?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("desktop preimage write: {e}"))?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("desktop preimage write: {e}"))?;
        f.sync_all()
            .map_err(|e| format!("desktop preimage write: {e}"))?;
        drop(f);
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("desktop preimage permissions: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("desktop preimage write: {e}"))?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("desktop preimage write: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("desktop preimage publish: {e}"))?;
    // Persist the directory entry before any settings write, like the
    // fresh publish above.
    sync_parent_dir(path)?;
    Ok(())
}

/// Load and validate the durable preimage file. Refuses on symlinks,
/// non-regular files, corrupt JSON, and hostile content — before any store
/// touch.
pub fn load_preimage_file_from(path: &std::path::Path) -> Result<DesktopPreimageFile, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| {
        "no desktop preimage; nothing to revert (run spaces-desktop-apply first)".to_owned()
    })?;
    if meta.file_type().is_symlink() {
        return Err("desktop preimage is a symlink; refusing".to_owned());
    }
    if !meta.is_file() {
        return Err("desktop preimage is not a regular file; refusing".to_owned());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|_| "desktop preimage unreadable; refusing".to_owned())?;
    let file: DesktopPreimageFile =
        serde_json::from_str(&raw).map_err(|_| "desktop preimage corrupt; refusing".to_owned())?;
    validate_preimage_file(&file)?;
    Ok(file)
}

/// Why a transaction lock acquisition refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LockRefusal {
    /// Another Apply/Revert holds the lock: refuse with zero writes,
    /// never wait or steal.
    Held,
    /// The lock could not be established: refuse before any store or
    /// backup touch.
    Refused(String),
}

/// Acquire the exclusive transaction lock for a leaf path, creating it
/// when absent. Portable core over stable std `File::try_lock`:
/// non-blocking (`WouldBlock` maps to [`LockRefusal::Held`]); holding the
/// returned `File` holds the lock, close-on-drop releases it, and the OS
/// releases on process loss, so a crash can never wedge later runs —
/// verified by probe on the pinned toolchain plus the release test below.
/// Secure leaf handling: symlink fast-path refusal plus a regular-file
/// check on both platforms, `O_NOFOLLOW` at open and mode `0600` on Unix.
/// No unsafe, no dependency. Non-mac live mutations still never reach here
/// (the SkyLight SPI is unavailable off macOS, so the live store refuses
/// per-ID before any publish); the portable core exists so the same
/// backup discipline is provable on every CI platform over fake temp
/// paths.
pub(crate) fn lock_leaf_file(path: &std::path::Path) -> Result<std::fs::File, LockRefusal> {
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        return Err(LockRefusal::Refused(
            "lock path is a symlink; refusing".to_owned(),
        ));
    }
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| LockRefusal::Refused(format!("lock open: {e}")))?
    };
    #[cfg(not(unix))]
    let file = {
        // No `O_NOFOLLOW` or Unix mode bits here: the symlink fast-path
        // above plus the regular-file check below still hold the leaf.
        // Read+write access is what the std lock needs on this platform.
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .map_err(|e| LockRefusal::Refused(format!("lock open: {e}")))?
    };
    if !file.metadata().map(|meta| meta.is_file()).unwrap_or(false) {
        return Err(LockRefusal::Refused(
            "lock path is not a regular file; refusing".to_owned(),
        ));
    }
    file.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => LockRefusal::Held,
        std::fs::TryLockError::Error(error) => {
            LockRefusal::Refused(format!("lock try_lock: {error}"))
        }
    })?;
    Ok(file)
}

/// Acquire the transaction lock for a preimage path, held across the full
/// read/publish/merge/write/remove sequence. Every refusal maps to a
/// plain error; callers return it before any store or backup touch, so a
/// locked-out run performs zero writes.
fn acquire_transaction_lock(preimage_path: &std::path::Path) -> Result<std::fs::File, String> {
    let lock_path = lock_path_for(preimage_path);
    lock_leaf_file(&lock_path).map_err(|e| match e {
        LockRefusal::Held => {
            "spaces-desktop command refused: another Apply/Revert holds the lock; zero writes performed"
                .to_owned()
        }
        LockRefusal::Refused(reason) => {
            format!(
                "spaces-desktop command refused: lock unestablishable ({reason}); zero writes performed"
            )
        }
    })
}

/// One applied desktop: the owned preimage state plus the write readback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyPerDesktop {
    pub symbolic_id: u32,
    pub desktop: u32,
    pub preimage: ShortcutEntry,
    pub applied: ShortcutEntry,
    pub readback_ok: bool,
    /// True when a previous Apply already owned this ID (verified, never
    /// rewritten).
    pub resumed: bool,
    /// The default expected chord for the default synthetic transfer
    /// (see [`default_expected_chord`]).
    pub expected_chord: Option<ShortcutEntry>,
    /// False when the owned chord differs from the default: preserved
    /// verbatim, but the product transfer must refuse the mismatch later
    /// rather than post a chord that does nothing.
    pub transfer_usable: bool,
}

/// One per-ID Apply failure: the stage that refused plus its cause. Every
/// partial failure is recoverable: the preimage file is retained and a
/// repeated Apply resumes from the stored first-observed preimages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyFailure {
    pub symbolic_id: u32,
    pub desktop: u32,
    pub stage: String,
    pub error: String,
}

/// Terminal Apply outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub results: Vec<ApplyPerDesktop>,
    pub failures: Vec<ApplyFailure>,
    /// IDs whose current value is neither the preimage nor applied: a
    /// later external edit, preserved and never overwritten.
    pub external_modified: Vec<u32>,
    /// One warning per result whose owned chord differs from the default:
    /// preserved verbatim, not usable for the default transfer.
    pub transfer_warnings: Vec<String>,
    pub preimage_path: Option<String>,
    /// True when a previous preimage file owned at least one ID.
    pub resumed: bool,
    /// Always true: enables are session-effective only (see
    /// [`SESSION_PERSISTENCE_NOTE`]).
    pub session_only: bool,
}

/// One warning for a desktop whose owned chord differs from the default
/// expected transfer chord.
fn transfer_warning(desktop: u32, symbolic_id: u32, observed: ShortcutEntry) -> String {
    format!(
        "desktop {desktop} (id {symbolic_id}): owned chord {observed:?} differs from the default Ctrl+digit chord; preserved verbatim, not usable for default transfer (product transfer must refuse this mismatch)"
    )
}

/// Explicit Apply: enable Desktops `1..=count` with owned preimages.
/// Ordering is fixed: validate, read all preimages (preferring stored
/// first-observed records when a preimage file already exists), publish
/// the durable preimage file (fresh IDs only — stored records are never
/// rewritten), then enable each entry with a readback. A publish failure
/// writes nothing; per-ID failures never block their siblings; a second
/// Apply resumes instead of overwriting the original backup.
pub fn apply_with_store_to<S: DesktopShortcutStore>(
    store: &mut S,
    count: u32,
    preimage_path: &std::path::Path,
) -> Result<ApplyOutcome, String> {
    if !(1..=APPLY_MAX_COUNT).contains(&count) {
        return Err(format!(
            "spaces-desktop-apply needs --count 1..={APPLY_MAX_COUNT}, got {count}"
        ));
    }
    // Transaction lock first: the full read/publish/merge/write sequence
    // below is exclusive across processes. Refusal returns before any
    // store or backup touch. The guard lives to the end of the function.
    let _lock = acquire_transaction_lock(preimage_path)?;
    // Resume base: the stored file's first-observed preimages win over
    // fresh reads, so a second Apply can never overwrite the original
    // owned backup. A hostile existing file refuses the whole run before
    // any write. The file order is preserved for deterministic merges.
    let existing: Option<DesktopPreimageFile> = match std::fs::symlink_metadata(preimage_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("desktop preimage metadata: {e}")),
        Ok(_) => Some(load_preimage_file_from(preimage_path)?),
    };
    let resumed = existing.is_some();
    let stored: HashMap<u32, DesktopPreimageRecord> = existing
        .as_ref()
        .map(|file| {
            file.desktops
                .iter()
                .map(|record| (record.symbolic_id, record.clone()))
                .collect()
        })
        .unwrap_or_default();
    // Phase 1: resolve every requested desktop. Stored records replay
    // their first-observed preimage; unstored IDs read fresh (refusals
    // become per-ID failures, never guesses).
    let mut planned: Vec<DesktopPreimageRecord> = Vec::new();
    let mut fresh_records: Vec<DesktopPreimageRecord> = Vec::new();
    let mut failures = Vec::new();
    let mut external_modified = Vec::new();
    for desktop in 1..=count {
        let symbolic_id = desktop_symbolic_id(desktop).expect("count capped to digit IDs");
        if let Some(record) = stored.get(&symbolic_id) {
            debug_assert_eq!(record.desktop, desktop);
            planned.push(record.clone());
            continue;
        }
        match store.read(symbolic_id) {
            Ok(preimage) => {
                let record = DesktopPreimageRecord {
                    symbolic_id,
                    desktop,
                    preimage,
                    applied: preimage.applied(),
                };
                fresh_records.push(record.clone());
                planned.push(record);
            }
            Err(error) => failures.push(ApplyFailure {
                symbolic_id,
                desktop,
                stage: "read".to_owned(),
                error,
            }),
        }
    }
    match existing {
        None => {
            if planned.is_empty() {
                return Ok(ApplyOutcome {
                    results: Vec::new(),
                    failures,
                    external_modified,
                    transfer_warnings: Vec::new(),
                    preimage_path: None,
                    resumed: false,
                    session_only: true,
                });
            }
            let file = DesktopPreimageFile {
                version: PREIMAGE_SCHEMA_VERSION,
                desktops: planned.clone(),
            };
            publish_preimage_file_to(preimage_path, &file)?;
        }
        Some(mut file) => {
            // Additive merge only: stored records are carried identically
            // (file order preserved), new observations append. A merge
            // refusal writes nothing further.
            if !fresh_records.is_empty() {
                file.desktops.append(&mut fresh_records);
                publish_merged_file_to(preimage_path, &file)?;
            }
        }
    }
    // Phase 2: enable each planned entry with a readback. Stored records
    // compare against their first-observed preimage; external edits
    // (neither preimage nor applied) are preserved, never overwritten.
    // Transfer usability rides the owned preimage chord (never rewritten).
    let mut results = Vec::new();
    for record in planned {
        let expected_chord = default_expected_chord(record.desktop);
        let transfer_usable = transfer_usable(record.desktop, record.preimage);
        let push_result = |results: &mut Vec<ApplyPerDesktop>| {
            results.push(ApplyPerDesktop {
                symbolic_id: record.symbolic_id,
                desktop: record.desktop,
                preimage: record.preimage,
                applied: record.applied,
                readback_ok: true,
                resumed,
                expected_chord,
                transfer_usable,
            });
        };
        let current = match store.read(record.symbolic_id) {
            Ok(current) => current,
            Err(error) => {
                failures.push(ApplyFailure {
                    symbolic_id: record.symbolic_id,
                    desktop: record.desktop,
                    stage: "read".to_owned(),
                    error,
                });
                continue;
            }
        };
        if current == record.applied {
            push_result(&mut results);
            continue;
        }
        if current != record.preimage {
            external_modified.push(record.symbolic_id);
            failures.push(ApplyFailure {
                symbolic_id: record.symbolic_id,
                desktop: record.desktop,
                stage: "external-preserved".to_owned(),
                error: format!(
                    "current {current:?} is neither the owned preimage nor applied; preserving the external edit"
                ),
            });
            continue;
        }
        if let Err(error) = store.set_enabled(record.symbolic_id, true) {
            failures.push(ApplyFailure {
                symbolic_id: record.symbolic_id,
                desktop: record.desktop,
                stage: "write".to_owned(),
                error,
            });
            continue;
        }
        match store.read(record.symbolic_id) {
            Ok(readback) if readback == record.applied => push_result(&mut results),
            Ok(readback) => failures.push(ApplyFailure {
                symbolic_id: record.symbolic_id,
                desktop: record.desktop,
                stage: "readback".to_owned(),
                error: format!("readback {readback:?} != applied {:?}", record.applied),
            }),
            Err(error) => failures.push(ApplyFailure {
                symbolic_id: record.symbolic_id,
                desktop: record.desktop,
                stage: "readback".to_owned(),
                error,
            }),
        }
    }
    let transfer_warnings: Vec<String> = results
        .iter()
        .filter(|result| !result.transfer_usable)
        .map(|result| transfer_warning(result.desktop, result.symbolic_id, result.preimage))
        .collect();
    Ok(ApplyOutcome {
        results,
        failures,
        external_modified,
        transfer_warnings,
        preimage_path: Some(preimage_path.to_string_lossy().into_owned()),
        resumed,
        session_only: true,
    })
}

/// How one desktop reverted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RevertKind {
    Restored,
    AlreadyRestored,
    KeptExternal,
}

/// One per-ID Revert result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevertPerDesktop {
    pub symbolic_id: u32,
    pub desktop: u32,
    pub kind: RevertKind,
    pub preimage_enabled: bool,
}

/// Terminal Revert outcome. The preimage file is removed only on a clean
/// full restore (no failures, no external edits); otherwise it is retained
/// for retry/audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevertOutcome {
    pub results: Vec<RevertPerDesktop>,
    pub failures: Vec<ApplyFailure>,
    pub external_modified: Vec<u32>,
    /// One warning per owned chord that differs from the default: restored
    /// verbatim, not usable for the default transfer.
    pub transfer_warnings: Vec<String>,
    pub preimage_retained: bool,
    pub preimage_path: Option<String>,
    /// Always true: enables are session-effective only (see
    /// [`SESSION_PERSISTENCE_NOTE`]).
    pub session_only: bool,
}

/// Explicit Revert: conditionally restore every owned preimage enabled
/// flag. Entries whose current value no longer equals what Apply wrote
/// are preserved and listed in `external_modified`, never overwritten.
pub fn revert_with_store_from<S: DesktopShortcutStore>(
    store: &mut S,
    preimage_path: &std::path::Path,
) -> Result<RevertOutcome, String> {
    // Transaction lock first: the full read/restore/remove sequence is
    // exclusive across processes. Refusal returns before any store or
    // backup touch. The guard lives to the end of the function.
    let _lock = acquire_transaction_lock(preimage_path)?;
    let file = load_preimage_file_from(preimage_path)?;
    let mut results = Vec::new();
    let mut failures = Vec::new();
    let mut external_modified = Vec::new();
    let mut transfer_warnings = Vec::new();
    for record in &file.desktops {
        if !transfer_usable(record.desktop, record.preimage) {
            transfer_warnings.push(transfer_warning(
                record.desktop,
                record.symbolic_id,
                record.preimage,
            ));
        }
        let current = match store.read(record.symbolic_id) {
            Ok(current) => current,
            Err(error) => {
                failures.push(ApplyFailure {
                    symbolic_id: record.symbolic_id,
                    desktop: record.desktop,
                    stage: "read".to_owned(),
                    error,
                });
                continue;
            }
        };
        match decide_restore(record.preimage, record.applied, current) {
            RestoreAction::AlreadyRestored => results.push(RevertPerDesktop {
                symbolic_id: record.symbolic_id,
                desktop: record.desktop,
                kind: RevertKind::AlreadyRestored,
                preimage_enabled: record.preimage.enabled,
            }),
            RestoreAction::KeepExternal => {
                external_modified.push(record.symbolic_id);
                results.push(RevertPerDesktop {
                    symbolic_id: record.symbolic_id,
                    desktop: record.desktop,
                    kind: RevertKind::KeptExternal,
                    preimage_enabled: record.preimage.enabled,
                });
            }
            RestoreAction::SetEnabled(enabled) => {
                match store.set_enabled(record.symbolic_id, enabled) {
                    Ok(()) => match store.read(record.symbolic_id) {
                        Ok(readback)
                            if readback.enabled == enabled
                                && readback.keycode == record.preimage.keycode
                                && readback.modifiers == record.preimage.modifiers =>
                        {
                            results.push(RevertPerDesktop {
                                symbolic_id: record.symbolic_id,
                                desktop: record.desktop,
                                kind: RevertKind::Restored,
                                preimage_enabled: record.preimage.enabled,
                            });
                        }
                        Ok(readback) => failures.push(ApplyFailure {
                            symbolic_id: record.symbolic_id,
                            desktop: record.desktop,
                            stage: "readback".to_owned(),
                            error: format!("readback {readback:?} != restored enabled={enabled}"),
                        }),
                        Err(error) => failures.push(ApplyFailure {
                            symbolic_id: record.symbolic_id,
                            desktop: record.desktop,
                            stage: "readback".to_owned(),
                            error,
                        }),
                    },
                    Err(error) => failures.push(ApplyFailure {
                        symbolic_id: record.symbolic_id,
                        desktop: record.desktop,
                        stage: "write".to_owned(),
                        error,
                    }),
                }
            }
        }
    }
    let clean = failures.is_empty() && external_modified.is_empty();
    let preimage_retained = if clean {
        match std::fs::remove_file(preimage_path) {
            Ok(()) => {
                // Persist the removal before reporting clean: a sync
                // refusal keeps the run failed (honest durability caveat)
                // with the file already unlinked — restores applied, so
                // there is nothing to retry, and `retained` stays false.
                if let Err(error) = sync_parent_dir(preimage_path) {
                    failures.push(ApplyFailure {
                        symbolic_id: 0,
                        desktop: 0,
                        stage: "cleanup".to_owned(),
                        error: format!(
                            "preimage removal dir-sync: {error} (removal itself succeeded)"
                        ),
                    });
                }
                false
            }
            Err(error) => {
                failures.push(ApplyFailure {
                    symbolic_id: 0,
                    desktop: 0,
                    stage: "cleanup".to_owned(),
                    error: format!("preimage removal: {error}"),
                });
                true
            }
        }
    } else {
        true
    };
    Ok(RevertOutcome {
        results,
        failures,
        external_modified,
        transfer_warnings,
        preimage_retained,
        preimage_path: Some(preimage_path.to_string_lossy().into_owned()),
        session_only: true,
    })
}

/// Read-only effective-table corroboration for the Desktop-N digit chords:
/// exactly which of `Ctrl+1..9,0` appear enabled in the effective table.
/// Corroboration only (Carbon entries carry no symbolic IDs); the bounded
/// switch experiment proves any mapping. Never indexed to symbolic IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopDigitCorroboration {
    /// Enabled desktops (1-10) with an exact `Ctrl+digit` effective chord.
    pub enabled: Vec<u32>,
    pub entries_seen: usize,
}

/// Pure digit corroboration over plain entries. No live touch.
#[must_use]
pub fn corroborate_desktop_digits(
    entries: &[crate::ffi::SymbolicHotKeyEntry],
) -> DesktopDigitCorroboration {
    let mut enabled = Vec::new();
    for desktop in 1..=10 {
        let Some(keycode) = desktop_digit_keycode(desktop) else {
            continue;
        };
        let hit = entries.iter().any(|entry| {
            entry.enabled
                && crate::symbolic_hotkeys::chord_matches(
                    entry.keycode,
                    entry.carbon_modifiers,
                    keycode,
                    crate::symbolic_hotkeys::CARBON_CTRL_BIT,
                )
        });
        if hit {
            enabled.push(desktop);
        }
    }
    DesktopDigitCorroboration {
        enabled,
        entries_seen: entries.len(),
    }
}

/// Live read-only effective-table digit corroboration. No IDs, no settings
/// reads or writes.
pub fn query_desktop_corroboration() -> Result<DesktopDigitCorroboration, String> {
    let entries = crate::ffi::copy_symbolic_hotkeys()?;
    Ok(corroborate_desktop_digits(&entries))
}

/// Production Apply over the live session store plus the canonical
/// preimage path. Startup and tile paths never call this: only the
/// explicit command does. A genuinely missing SPI refuses narrowly per ID
/// with zero writes.
pub fn run_apply(count: u32) -> Result<String, (i32, String)> {
    ensure_private_recovery_dir().map_err(|e| (2, e))?;
    let mut store = LiveStore::new();
    let Some(path) = canonical_preimage_path() else {
        return Err((
            2,
            "desktop recovery: no $HOME; refusing (no temporary fallback)".to_owned(),
        ));
    };
    let envelope = |outcome: &ApplyOutcome| {
        serde_json::json!({
            "outcome": outcome,
            "persistence": SESSION_PERSISTENCE_NOTE,
            "live_evidence": "session SPI reads/writes with per-ID readback; no plist touched",
        })
        .to_string()
    };
    match apply_with_store_to(&mut store, count, &path) {
        Ok(outcome) if outcome.failures.is_empty() => {
            serde_json::to_string(&outcome).map_err(|e| (1, e.to_string()))
        }
        Ok(outcome) => Err((1, envelope(&outcome))),
        Err(error) => Err((2, error)),
    }
}

/// Production Revert over the live session store plus the canonical
/// preimage path.
pub fn run_revert() -> Result<String, (i32, String)> {
    ensure_private_recovery_dir().map_err(|e| (2, e))?;
    let mut store = LiveStore::new();
    let Some(path) = canonical_preimage_path() else {
        return Err((
            2,
            "desktop recovery: no $HOME; refusing (no temporary fallback)".to_owned(),
        ));
    };
    let envelope = |outcome: &RevertOutcome| {
        serde_json::json!({
            "outcome": outcome,
            "persistence": SESSION_PERSISTENCE_NOTE,
            "live_evidence": "session SPI reads/writes with per-ID readback; no plist touched",
        })
        .to_string()
    };
    match revert_with_store_from(&mut store, &path) {
        Ok(outcome) if outcome.failures.is_empty() && outcome.external_modified.is_empty() => {
            serde_json::to_string(&outcome).map_err(|e| (1, e.to_string()))
        }
        Ok(outcome) => Err((1, envelope(&outcome))),
        Err(error) => Err((1, error)),
    }
}

/// Production read-only status: durable preimage presence, SPI
/// availability, plus live effective-table digit corroboration. No
/// settings reads or writes beyond the public effective table.
pub fn run_status() -> Result<String, (i32, String)> {
    // Read-only: never creates the recovery dir, never fails on a missing
    // home — absence just reports no preimage.
    let path = canonical_preimage_path();
    let preimage = path.as_ref().and_then(|p| load_preimage_file_from(p).ok());
    let corroboration = query_desktop_corroboration().map_err(|e| (1, e.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "preimage_present": preimage.is_some(),
        "preimage_path": path.as_ref().map(|p| p.to_string_lossy()),
        "preimage_desktops": preimage.map(|f| f.desktops.iter().map(|r| r.desktop).collect::<Vec<_>>()),
        "spi_available": crate::ffi::hotkey_spi_available(),
        "digit_corroboration": corroboration,
        "persistence": SESSION_PERSISTENCE_NOTE,
        "mutates": false,
    }))
    .map_err(|e| (1, e.to_string()))
}
