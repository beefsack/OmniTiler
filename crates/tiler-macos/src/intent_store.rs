#![forbid(unsafe_code)]

//! Adapter-owned durable host store for retained opaque window intent
//! (`intentDesired`).
//!
//! The core owns the opaque intent record and publishes one authoritative
//! current target per window lifetime ([`tiler_core::world_report::IntentDesire`]);
//! durability stays adapter-local ([contract](../../../docs/decisions/contract.md#intent-ownership-and-persistence),
//! [persistence](../../../docs/decisions/persistence.md#orderly-restart-adoption)).
//! This module is the macOS slot: one private per-user file under
//! `~/Library/Application Support/OmniTiler/`, named
//! `intent-v1-<uid>-<asid>.json` for the current login namespace
//! (`getaudit_addr` UID + audit session id, see [`crate::ffi::audit_self`]).
//!
//! Hardening contract:
//! - directory mode `0700`, owned by the current UID, never a symlink;
//! - file mode `0600`, owned by the current UID, never a symlink
//!   (`O_NOFOLLOW`), bounded to 4 MiB and 10,000 rows;
//! - each row binds `(win, wid, pid, launch_date_ns, bundle_id, data)`
//!   where `win == "<pid>_<wid>"`, `launch_date_ns` is the numeric
//!   `NSRunningApplication.launchDate` (seconds since 2001 converted to
//!   nanoseconds, see [`launch_date_ns_from_bits`]) and `bundle_id` the
//!   LaunchServices bundle identity;
//! - `data` is opaque to the store: it is persisted verbatim under the
//!   [`MAX_DATA_BYTES`] bound and never decoded here, so the core keeps
//!   sole ownership of the record encoding;
//! - atomic replace through a same-directory temp file, `fsync`, then
//!   `rename`; writes are serialized by the owning engine thread.
//!
//! A corrupt, unreadable, over-bound, wrong-UID, wrong-namespace, or
//! symlinked store degrades to empty with one bounded diagnostic string.
//! It never guesses an entry, and never deletes a persisted row merely
//! because the core did not publish it (absence carries no authority).

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::runtime_types::{parse_window_id, window_id};

/// Environment override for the store directory (native fixture isolation
/// and tests). The file name still carries the live login namespace.
pub(crate) const STORE_DIR_ENV: &str = "OMNITILER_INTENT_STORE_DIR";

/// Store file schema version (versioned envelope; a future shape gets a new
/// file family instead of silently reinterpreting this one).
pub(crate) const STORE_VERSION: u32 = 1;

/// Bound on the whole store file (matches the world-contract 4 MiB bound).
pub(crate) const MAX_STORE_FILE_BYTES: usize = 4 * 1024 * 1024;
/// Bound on persisted rows.
pub(crate) const MAX_STORE_ENTRIES: usize = 10_000;
/// Bound on one opaque intent data string (UTF-8 bytes).
pub(crate) const MAX_DATA_BYTES: usize = 1024;
/// Bound on one bundle identity string (matches metadata text bounds).
pub(crate) const MAX_BUNDLE_ID_BYTES: usize = 4096;
/// Canonical window id bound (`"<u32>_<u32>"`).
pub(crate) const MAX_WIN_BYTES: usize = 32;
/// Bounded diagnostic text.
const MAX_DIAGNOSTIC_BYTES: usize = 200;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One verified live process lifetime, as read from the OS at binding time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveBinding {
    pub launch_date_ns: u64,
    pub bundle_id: String,
}

/// Injected live-lifetime probe (tests supply deterministic bindings; the
/// production session reads the OS through [`live_binding`]).
pub(crate) type IdentityProbe = Box<dyn Fn(u32) -> Option<LiveBinding> + Send>;

/// Injected process-liveness probe (tests supply deterministic liveness;
/// the production session reads [`live_process`]).
pub(crate) type ProcessProbe = Box<dyn Fn(u32) -> bool + Send>;

/// Whether `pid` currently names a live process. Liveness alone carries no
/// lifetime authority: it only distinguishes a gone process (recoverable
/// hold) from a live process whose lifetime binding is unreadable (skip the
/// row, never guess a binding).
pub(crate) fn live_process(pid: u32) -> bool {
    crate::ffi::process_exists(pid as i32)
}

impl crate::runtime_types::IntentStoreMode {
    /// Resolve this mode to a loaded store. `Disabled` is always `None`;
    /// every failure path degrades to `None` with one bounded diagnostic, so
    /// no store means no persistence and never an accidental path.
    pub(crate) fn open(&self) -> Option<IntentStore> {
        match self {
            Self::Disabled => None,
            Self::Default => IntentStore::open_default(),
            Self::Isolated(dir) => {
                let (uid, asid) = live_namespace()?;
                let store = IntentStore::open_at(dir.clone(), uid, asid);
                if let Some(diagnostic) = store.diagnostic() {
                    eprintln!("intent-store: {diagnostic}");
                }
                Some(store)
            }
        }
    }
}

/// Live login namespace, or `None` with one bounded diagnostic when the
/// `getaudit_addr` pair is unavailable (fail closed, never a guessed pair).
fn live_namespace() -> Option<(u32, i32)> {
    match crate::ffi::audit_self() {
        Ok(namespace) => Some(namespace),
        Err(error) => {
            eprintln!("intent-store: session namespace unavailable ({error})");
            None
        }
    }
}

/// Read the live `(launch, bundle)` binding for `pid`. Both legs are
/// required: an app without a LaunchServices entry has no durable window
/// lifetime identity, so it is never persisted or imported.
pub(crate) fn live_binding(pid: u32) -> Option<LiveBinding> {
    if pid <= 1 {
        return None;
    }
    let identity = crate::ffi::ns_app_identity_for_pid(pid as i32);
    let bundle_id = identity.bundle.filter(|bundle| !bundle.is_empty())?;
    if bundle_id.len() > MAX_BUNDLE_ID_BYTES {
        return None;
    }
    let launch_date_ns = launch_date_ns_from_bits(identity.launch_ref_bits?)?;
    Some(LiveBinding {
        launch_date_ns,
        bundle_id,
    })
}

/// Convert the exact numeric `NSDate.timeIntervalSinceReferenceDate` bits
/// (seconds since 2001-01-01 UTC) into integer nanoseconds. `None` for a
/// non-finite or out-of-range reading. Both persistence and verification
/// run through this one conversion, so a stable numeric launch date
/// round-trips exactly.
pub(crate) fn launch_date_ns_from_bits(bits: u64) -> Option<u64> {
    let secs = f64::from_bits(bits);
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    let ns = (secs * 1_000_000_000.0).round();
    if !ns.is_finite() || ns < 0.0 || ns > (u64::MAX as f64) {
        return None;
    }
    Some(ns as u64)
}

/// One durable row: the storage-verified live subject plus the exact opaque
/// record bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredEntry {
    pub win: String,
    pub wid: u32,
    pub pid: u32,
    pub launch_date_ns: u64,
    pub bundle_id: String,
    pub data: String,
}

impl StoredEntry {
    /// Whether this row's lifetime binding is exactly the live binding
    /// read for `(pid, wid)`.
    pub(crate) fn matches_live(&self, pid: u32, wid: u32, live: Option<&LiveBinding>) -> bool {
        self.pid == pid
            && self.wid == wid
            && live.is_some_and(|live| {
                live.launch_date_ns == self.launch_date_ns && live.bundle_id == self.bundle_id
            })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreFile {
    version: u32,
    session_uid: u32,
    session_asid: i32,
    entries: Vec<StoredEntryJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEntryJson {
    win: String,
    wid: u32,
    pid: u32,
    launch_date_ns: u64,
    bundle_id: String,
    data: String,
}

/// Adapter-owned intent store bound to one login namespace. Rows are keyed
/// by the runtime window id; [`IntentStore::durable`] remembers the last
/// successfully persisted value per window, including an explicit clear
/// (`None`), so equality is adapter-local and costs zero writes.
pub(crate) struct IntentStore {
    dir: PathBuf,
    uid: u32,
    asid: i32,
    /// False when the supplied/requested namespace disagrees with the live
    /// `getaudit_addr` pair: the store then refuses every read and write.
    namespace_ok: bool,
    entries: BTreeMap<String, StoredEntry>,
    durable: BTreeMap<String, Option<String>>,
    diagnostic: Option<String>,
}

impl IntentStore {
    /// Production store for the current login, or `None` when `$HOME` or
    /// the audit-session namespace is unavailable (fail closed: no
    /// persistence, never a guessed directory).
    pub(crate) fn open_default() -> Option<Self> {
        let dir = default_dir()?;
        let (uid, asid) = live_namespace()?;
        let store = Self::open_at(dir, uid, asid);
        if let Some(diagnostic) = store.diagnostic() {
            eprintln!("intent-store: {diagnostic}");
        }
        Some(store)
    }

    /// Open (and load) the store for an explicit namespace. The namespace
    /// must equal the live `getaudit_addr` pair or the store degrades to a
    /// refusing empty store.
    pub(crate) fn open_at(dir: PathBuf, uid: u32, asid: i32) -> Self {
        let namespace_ok = matches!(
            crate::ffi::audit_self(),
            Ok((live_uid, live_asid)) if live_uid == uid && live_asid == asid
        );
        let mut store = Self {
            dir,
            uid,
            asid,
            namespace_ok,
            entries: BTreeMap::new(),
            durable: BTreeMap::new(),
            diagnostic: None,
        };
        if namespace_ok {
            store.load();
        } else {
            store.diagnose("session namespace mismatch");
        }
        store
    }

    /// Test-only constructor that skips the live-audit comparison so an
    /// isolated temporary directory can be exercised on any host.
    #[cfg(test)]
    pub(crate) fn open_at_unchecked(dir: PathBuf, uid: u32, asid: i32) -> Self {
        let mut store = Self {
            dir,
            uid,
            asid,
            namespace_ok: true,
            entries: BTreeMap::new(),
            durable: BTreeMap::new(),
            diagnostic: None,
        };
        store.load();
        store
    }

    /// Persisted rows loaded or last written, in stable window order.
    pub(crate) fn entries(&self) -> &BTreeMap<String, StoredEntry> {
        &self.entries
    }

    /// Last successfully persisted value per window (`None` is a proven
    /// clear). Kept in memory for adapter-local equality.
    pub(crate) fn durable(&self) -> &BTreeMap<String, Option<String>> {
        &self.durable
    }

    /// One bounded diagnostic when the store degraded to empty; `None` for a
    /// clean load (including the ordinary missing-file first run).
    pub(crate) fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    /// Store file name for this namespace.
    pub(crate) fn file_name(&self) -> String {
        format!("intent-v{STORE_VERSION}-{}-{}.json", self.uid, self.asid)
    }

    /// Full store file path.
    pub(crate) fn path(&self) -> PathBuf {
        self.dir.join(self.file_name())
    }

    /// Store directory (exactly the one this store writes into).
    pub(crate) fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Apply a batch of row replacements atomically: `Some` inserts/replaces,
    /// `None` removes the persisted row. The complete snapshot is written
    /// through temp + fsync + rename; on failure nothing in memory changes,
    /// so a later accepted reply retries the same difference.
    pub(crate) fn apply_changes(
        &mut self,
        changes: BTreeMap<String, Option<StoredEntry>>,
    ) -> Result<(), String> {
        if !self.namespace_ok {
            return Err("intent store session namespace mismatch".to_owned());
        }
        let mut next = self.entries.clone();
        for (win, change) in &changes {
            match change {
                Some(entry) => {
                    validate_entry(entry)?;
                    next.insert(win.clone(), entry.clone());
                }
                None => {
                    next.remove(win);
                }
            }
        }
        validate_entries(&next)?;
        let bytes = encode_snapshot(self, &next)?;
        self.ensure_dir()?;
        self.check_existing_file()?;
        self.write_atomic(&bytes)?;
        self.entries = next;
        for (win, change) in changes {
            self.durable.insert(win, change.map(|entry| entry.data));
        }
        Ok(())
    }

    fn diagnose(&mut self, reason: &str) {
        if self.diagnostic.is_some() {
            return;
        }
        let mut bounded: String = reason.chars().take(MAX_DIAGNOSTIC_BYTES).collect();
        if bounded.is_empty() {
            bounded.push_str("intent store unavailable");
        }
        self.diagnostic = Some(bounded);
    }

    fn load(&mut self) {
        match std::fs::symlink_metadata(&self.dir) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    self.diagnose("store directory is a symlink");
                    return;
                }
                if !meta.is_dir() {
                    self.diagnose("store directory is not a directory");
                    return;
                }
                if meta.uid() != self.uid {
                    self.diagnose("store directory owner mismatch");
                    return;
                }
                if meta.permissions().mode() & 0o777 != 0o700 {
                    self.diagnose("store directory mode is not 0700");
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => {
                self.diagnose("store directory unreadable");
                return;
            }
        }
        let path = self.path();
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => {
                self.diagnose("store file unreadable or a symlink");
                return;
            }
        };
        let meta = match file.metadata() {
            Ok(meta) => meta,
            Err(_) => {
                self.diagnose("store file metadata unreadable");
                return;
            }
        };
        if !meta.is_file() {
            self.diagnose("store path is not a regular file");
            return;
        }
        if meta.uid() != self.uid {
            self.diagnose("store file owner mismatch");
            return;
        }
        if meta.permissions().mode() & 0o777 != 0o600 {
            self.diagnose("store file mode is not 0600");
            return;
        }
        if meta.len() > MAX_STORE_FILE_BYTES as u64 {
            self.diagnose("store file exceeds size bound");
            return;
        }
        let mut bytes = Vec::new();
        let mut limited = file.take(MAX_STORE_FILE_BYTES as u64 + 1);
        if limited.read_to_end(&mut bytes).is_err() {
            self.diagnose("store file read failed");
            return;
        }
        if bytes.len() > MAX_STORE_FILE_BYTES {
            self.diagnose("store file exceeds size bound");
            return;
        }
        match decode_snapshot(self, &bytes) {
            Ok(entries) => {
                for entry in entries.values() {
                    self.durable
                        .insert(entry.win.clone(), Some(entry.data.clone()));
                }
                self.entries = entries;
            }
            Err(reason) => self.diagnose(&reason),
        }
    }

    fn ensure_dir(&self) -> Result<(), String> {
        match std::fs::symlink_metadata(&self.dir) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err("intent store directory is a symlink".to_owned());
                }
                if !meta.is_dir() {
                    return Err("intent store directory is not a directory".to_owned());
                }
                if meta.uid() != self.uid {
                    return Err("intent store directory owner mismatch".to_owned());
                }
                if meta.permissions().mode() & 0o777 != 0o700 {
                    return Err("intent store directory mode is not 0700".to_owned());
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(&self.dir)
                    .map_err(|error| format!("intent store directory create: {error}"))?;
                std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o700))
                    .map_err(|error| format!("intent store directory mode: {error}"))?;
                let meta = std::fs::symlink_metadata(&self.dir)
                    .map_err(|error| format!("intent store directory metadata: {error}"))?;
                if meta.file_type().is_symlink()
                    || !meta.is_dir()
                    || meta.uid() != self.uid
                    || meta.permissions().mode() & 0o777 != 0o700
                {
                    return Err("intent store directory verification failed".to_owned());
                }
                Ok(())
            }
            Err(error) => Err(format!("intent store directory metadata: {error}")),
        }
    }

    fn check_existing_file(&self) -> Result<(), String> {
        match std::fs::symlink_metadata(self.path()) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err("intent store file is a symlink".to_owned());
                }
                if !meta.is_file() {
                    return Err("intent store path is not a regular file".to_owned());
                }
                if meta.uid() != self.uid {
                    return Err("intent store file owner mismatch".to_owned());
                }
                if meta.permissions().mode() & 0o777 != 0o600 {
                    return Err("intent store file mode is not 0600".to_owned());
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("intent store file metadata: {error}")),
        }
    }

    fn write_atomic(&self, bytes: &[u8]) -> Result<(), String> {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp = self.dir.join(format!(
            ".{}.tmp.{}.{counter}",
            self.file_name(),
            std::process::id()
        ));
        let result = (|| -> Result<(), String> {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&temp)
                .map_err(|error| format!("intent store temp open: {error}"))?;
            // Defeat a restrictive umask: the store file is exactly 0600.
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("intent store temp mode: {error}"))?;
            let mut file = file;
            file.write_all(bytes)
                .map_err(|error| format!("intent store temp write: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("intent store temp fsync: {error}"))?;
            drop(file);
            std::fs::rename(&temp, self.path())
                .map_err(|error| format!("intent store rename: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        } else if let Ok(dir) = File::open(&self.dir) {
            // Directory fsync is best-effort: the file itself is already
            // durable, and some filesystems refuse directory fsync.
            let _ = dir.sync_all();
        }
        result
    }
}

use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

fn default_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(STORE_DIR_ENV)
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    // Same per-user application-support directory as the settings and
    // shortcut-recovery files; one convention, never a second path.
    crate::desktop_shortcuts::recovery_dir()
}

fn validate_entry(entry: &StoredEntry) -> Result<(), String> {
    let Some((pid, wid)) = parse_window_id(&entry.win) else {
        return Err("intent store row has a malformed window id".to_owned());
    };
    if pid != entry.pid || wid != entry.wid || window_id(pid, wid) != entry.win {
        return Err("intent store row binding does not match its window id".to_owned());
    }
    if entry.pid <= 1 || entry.wid == 0 {
        return Err("intent store row has an invalid process/window id".to_owned());
    }
    if entry.win.len() > MAX_WIN_BYTES {
        return Err("intent store row window id exceeds bound".to_owned());
    }
    if entry.launch_date_ns == 0 {
        return Err("intent store row has no launch date".to_owned());
    }
    if entry.bundle_id.is_empty() || entry.bundle_id.len() > MAX_BUNDLE_ID_BYTES {
        return Err("intent store row bundle id is empty or overbound".to_owned());
    }
    if entry.data.len() > MAX_DATA_BYTES {
        return Err("intent store row data exceeds bound".to_owned());
    }
    Ok(())
}

fn validate_entries(entries: &BTreeMap<String, StoredEntry>) -> Result<(), String> {
    if entries.len() > MAX_STORE_ENTRIES {
        return Err("intent store exceeds row bound".to_owned());
    }
    for entry in entries.values() {
        validate_entry(entry)?;
    }
    Ok(())
}

fn encode_snapshot(
    store: &IntentStore,
    entries: &BTreeMap<String, StoredEntry>,
) -> Result<Vec<u8>, String> {
    let file = StoreFile {
        version: STORE_VERSION,
        session_uid: store.uid,
        session_asid: store.asid,
        entries: entries
            .values()
            .map(|entry| StoredEntryJson {
                win: entry.win.clone(),
                wid: entry.wid,
                pid: entry.pid,
                launch_date_ns: entry.launch_date_ns,
                bundle_id: entry.bundle_id.clone(),
                data: entry.data.clone(),
            })
            .collect(),
    };
    let bytes =
        serde_json::to_vec(&file).map_err(|error| format!("intent store encode: {error}"))?;
    if bytes.len() > MAX_STORE_FILE_BYTES {
        return Err("intent store snapshot exceeds size bound".to_owned());
    }
    Ok(bytes)
}

fn parse_store_file(bytes: &[u8]) -> Result<StoreFile, String> {
    if bytes.len() > MAX_STORE_FILE_BYTES {
        return Err("store file exceeds size bound".to_owned());
    }
    let file: StoreFile =
        serde_json::from_slice(bytes).map_err(|_| "store file is malformed".to_owned())?;
    if file.version != STORE_VERSION {
        return Err("store file version mismatch".to_owned());
    }
    if file.entries.len() > MAX_STORE_ENTRIES {
        return Err("store file exceeds row bound".to_owned());
    }
    Ok(file)
}

fn entries_from_file(file: StoreFile) -> Result<BTreeMap<String, StoredEntry>, String> {
    let mut entries = BTreeMap::new();
    for raw in file.entries {
        let entry = StoredEntry {
            win: raw.win,
            wid: raw.wid,
            pid: raw.pid,
            launch_date_ns: raw.launch_date_ns,
            bundle_id: raw.bundle_id,
            data: raw.data,
        };
        validate_entry(&entry)?;
        if entries.insert(entry.win.clone(), entry).is_some() {
            return Err("store file repeats a window id".to_owned());
        }
    }
    Ok(entries)
}

fn decode_snapshot(
    store: &IntentStore,
    bytes: &[u8],
) -> Result<BTreeMap<String, StoredEntry>, String> {
    let file = parse_store_file(bytes)?;
    if file.session_uid != store.uid || file.session_asid != store.asid {
        return Err("store file session namespace mismatch".to_owned());
    }
    entries_from_file(file)
}

/// Decode one persisted store envelope into its exact rows without opening
/// the live namespace or touching disk. The caller already owns the bytes
/// (for example an isolated-store fingerprint) and is responsible for the
/// namespace implied by the store directory. Validation is the same row and
/// envelope validation used by the live loader; no row is guessed.
pub(crate) fn decode_store_entries(bytes: &[u8]) -> Result<Vec<StoredEntry>, String> {
    entries_from_file(parse_store_file(bytes)?).map(|entries| entries.into_values().collect())
}

#[cfg(test)]
pub(crate) struct TestRoot {
    pub(crate) root: PathBuf,
    pub(crate) dir: PathBuf,
}

#[cfg(test)]
impl TestRoot {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "omnitiler-intent-test-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("create isolated intent test root");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
            .expect("set isolated intent test root mode");
        let dir = root.join("OmniTiler");
        Self { root, dir }
    }

    pub(crate) fn dir(&self) -> PathBuf {
        self.dir.clone()
    }
}

#[cfg(test)]
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Isolated temporary store plus its cleanup guard. The store can be moved
/// into a session (via [`test_store_parts`]) while the root guard still
/// removes the whole tree at test end.
#[cfg(test)]
pub(crate) fn test_store_parts() -> (TestRoot, IntentStore) {
    let root = TestRoot::new();
    let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
    let store = IntentStore::open_at_unchecked(root.dir(), uid, asid);
    (root, store)
}

#[cfg(test)]
pub(crate) struct TestStore {
    pub(crate) root: TestRoot,
    pub(crate) store: IntentStore,
}

#[cfg(test)]
impl TestStore {
    /// Isolated temporary store (never the user's Application Support).
    pub(crate) fn new() -> Self {
        let (root, store) = test_store_parts();
        Self { root, store }
    }

    pub(crate) fn dir(&self) -> PathBuf {
        self.store.dir.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(win: &str, data: &str) -> StoredEntry {
        let (pid, wid) = parse_window_id(win).expect("canonical window id");
        StoredEntry {
            win: win.to_owned(),
            wid,
            pid,
            launch_date_ns: 123_456_789,
            bundle_id: "com.example.app".to_owned(),
            data: data.to_owned(),
        }
    }

    fn canonical(float: Option<bool>) -> String {
        tiler_core::intent_data::encode_intent_data(&tiler_core::intent_data::IntentData::new(
            float, None,
        ))
    }

    fn snapshot(store: &TestStore) -> (Vec<u8>, std::time::SystemTime) {
        let path = store.store.path();
        (
            std::fs::read(&path).expect("read store"),
            std::fs::metadata(&path)
                .expect("store metadata")
                .modified()
                .expect("store mtime"),
        )
    }

    #[test]
    fn arbitrary_bounded_opaque_data_round_trips_without_decoding() {
        let mut fixture = TestStore::new();
        // Not a valid intent record: the store treats `data` as opaque.
        let data = "opaque payload {\"not\":\"intent\"} \u{1F600}".to_owned();
        fixture
            .store
            .apply_changes(BTreeMap::from([(
                "4242_7".to_owned(),
                Some(entry("4242_7", &data)),
            )]))
            .expect("persist opaque data");
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert_eq!(reopened.diagnostic(), None);
        assert_eq!(
            reopened
                .entries()
                .get("4242_7")
                .map(|row| row.data.as_str()),
            Some(data.as_str())
        );
        assert_eq!(reopened.durable().get("4242_7"), Some(&Some(data)));
    }

    #[test]
    fn store_mode_defaults_disabled_and_isolated_opens_exact_dir() {
        use crate::runtime_types::IntentStoreMode;
        assert_eq!(IntentStoreMode::default(), IntentStoreMode::Disabled);
        assert!(IntentStoreMode::Disabled.open().is_none());

        let fixture = TestRoot::new();
        let dir = fixture.dir();
        if crate::ffi::audit_self().is_err() {
            // No live login namespace here: the store refuses closed rather
            // than guessing a namespace.
            assert!(IntentStoreMode::Isolated(dir).open().is_none());
            return;
        }
        let mut store = IntentStoreMode::Isolated(dir.clone())
            .open()
            .expect("isolated store opens under the live namespace");
        assert_eq!(store.dir(), dir.as_path());
        let data = canonical(Some(true));
        store
            .apply_changes(BTreeMap::from([(
                "4242_7".to_owned(),
                Some(entry("4242_7", &data)),
            )]))
            .expect("write lands in the isolated dir");
        let path = dir.join(store.file_name());
        assert!(
            path.exists(),
            "write lands inside the isolated dir: {path:?}"
        );
    }

    #[test]
    fn store_path_permissions_and_atomic_write() {
        let mut fixture = TestStore::new();
        let data = canonical(Some(true));
        fixture
            .store
            .apply_changes(BTreeMap::from([(
                "4242_7".to_owned(),
                Some(entry("4242_7", &data)),
            )]))
            .expect("persist one row");
        let path = fixture.store.path();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(fixture.store.file_name().as_str())
        );
        assert_eq!(fixture.store.file_name(), {
            let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
            format!("intent-v1-{uid}-{asid}.json")
        });
        let dir_meta = std::fs::symlink_metadata(fixture.dir()).expect("dir metadata");
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);
        let file_meta = std::fs::symlink_metadata(&path).expect("file metadata");
        assert_eq!(file_meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(file_meta.uid(), dir_meta.uid());
        let leftovers: Vec<_> = std::fs::read_dir(fixture.dir())
            .expect("list dir")
            .filter_map(Result::ok)
            .filter(|row| row.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "no temp files survive a write");
        assert_eq!(
            fixture.store.durable().get("4242_7"),
            Some(&Some(data.clone()))
        );
        // Reload through a fresh store instance.
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert_eq!(
            reopened.entries().get("4242_7"),
            Some(&entry("4242_7", &data))
        );
        assert_eq!(reopened.diagnostic(), None);
    }

    #[test]
    fn missing_store_is_clean_and_empty() {
        let fixture = TestStore::new();
        assert!(fixture.store.entries().is_empty());
        assert!(fixture.store.durable().is_empty());
        assert_eq!(fixture.store.diagnostic(), None);
        assert!(!fixture.store.path().exists());
    }

    #[test]
    fn corrupt_store_degrades_to_empty() {
        for bytes in [
            b"not json".to_vec(),
            br#"{"version":2,"session_uid":0,"session_asid":0,"entries":[]}"#.to_vec(),
            br#"{"version":1,"session_uid":0,"session_asid":0,"entries":[],"extra":1}"#.to_vec(),
            br#"{"version":1,"session_uid":0,"session_asid":0,"entries":[{"win":"4242_7","wid":7,"pid":4242,"launch_date_ns":0,"bundle_id":"com.example.app","data":"{\"v\":1,\"float\":true,\"stickyPrior\":null}"}]}"#.to_vec(),
        ] {
            let fixture = TestStore::new();
            std::fs::create_dir_all(fixture.dir()).expect("dir");
            std::fs::set_permissions(
                fixture.dir(),
                std::fs::Permissions::from_mode(0o700),
            )
            .expect("dir mode");
            let path = fixture.store.path();
            std::fs::write(&path, &bytes).expect("write corrupt store");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("file mode");
            let reopened = IntentStore::open_at_unchecked(
                fixture.dir(),
                fixture.store.uid,
                fixture.store.asid,
            );
            assert!(reopened.entries().is_empty(), "degrade to empty");
            assert!(reopened.durable().is_empty());
            assert!(reopened.diagnostic().is_some(), "bounded diagnostic");
            assert!(
                reopened
                    .diagnostic()
                    .is_some_and(|text| text.len() <= MAX_DIAGNOSTIC_BYTES)
            );
        }
    }

    #[test]
    fn wrong_session_and_wrong_uid_degrade_to_empty() {
        let mut fixture = TestStore::new();
        let data = canonical(Some(true));
        fixture
            .store
            .apply_changes(BTreeMap::from([(
                "4242_7".to_owned(),
                Some(entry("4242_7", &data)),
            )]))
            .expect("persist");
        let bytes = std::fs::read(fixture.store.path()).expect("read");
        // Wrong internal session namespace.
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        let wrong_session = text.replace(
            "\"session_asid\":",
            &format!("\"session_asid\":{}", fixture.store.asid.saturating_add(1)),
        );
        std::fs::write(fixture.store.path(), wrong_session).expect("rewrite");
        std::fs::set_permissions(fixture.store.path(), std::fs::Permissions::from_mode(0o600))
            .expect("mode");
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert!(reopened.entries().is_empty());
        assert!(reopened.diagnostic().is_some());
        // Wrong expected UID: the real file is owned by the live user, so a
        // different expected UID rejects the metadata check.
        let uid = fixture.store.uid;
        let asid = fixture.store.asid;
        let fake_uid = uid.saturating_add(1);
        let fake_name = format!("intent-v1-{fake_uid}-{asid}.json");
        let mut file = serde_json::from_slice::<serde_json::Value>(&bytes).expect("json");
        file["session_uid"] = serde_json::json!(fake_uid);
        let fake_bytes = serde_json::to_vec(&file).expect("encode");
        std::fs::write(fixture.dir().join(fake_name), fake_bytes).expect("write fake uid");
        std::fs::set_permissions(
            fixture
                .dir()
                .join(format!("intent-v1-{fake_uid}-{asid}.json")),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("mode");
        let reopened = IntentStore::open_at_unchecked(fixture.dir(), fake_uid, asid);
        assert!(reopened.entries().is_empty());
        assert!(
            reopened
                .diagnostic()
                .is_some_and(|text| text.contains("owner")),
            "uid mismatch diagnostic: {:?}",
            reopened.diagnostic()
        );
    }

    #[test]
    fn symlinked_file_and_directory_are_rejected() {
        let mut fixture = TestStore::new();
        let data = canonical(Some(true));
        fixture
            .store
            .apply_changes(BTreeMap::from([(
                "4242_7".to_owned(),
                Some(entry("4242_7", &data)),
            )]))
            .expect("persist");
        let path = fixture.store.path();
        let moved = fixture.root.root.join("moved.json");
        std::fs::rename(&path, &moved).expect("move");
        std::os::unix::fs::symlink(&moved, &path).expect("symlink file");
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert!(reopened.entries().is_empty());
        assert!(reopened.diagnostic().is_some());
        let mut refusing = reopened;
        assert!(
            refusing
                .apply_changes(BTreeMap::from([(
                    "4242_7".to_owned(),
                    Some(entry("4242_7", &data)),
                )]))
                .is_err(),
            "never replace through a symlink"
        );

        // Symlinked directory.
        let fixture = TestStore::new();
        let real = fixture.root.root.join("real-store");
        std::fs::create_dir_all(&real).expect("real dir");
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).expect("mode");
        std::os::unix::fs::symlink(&real, fixture.dir()).expect("symlink dir");
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert!(reopened.entries().is_empty());
        assert!(
            reopened
                .diagnostic()
                .is_some_and(|text| text.contains("symlink"))
        );
    }

    #[test]
    fn overbound_store_degrades_and_refuses() {
        let mut fixture = TestStore::new();
        // Data over the 1024-byte inner bound.
        let over_data = format!(
            "{{\"v\":1,\"float\":null,\"stickyPrior\":null}}{}",
            "x".repeat(1100)
        );
        let mut bad = entry("4242_7", &over_data);
        bad.data = over_data;
        assert!(
            fixture
                .store
                .apply_changes(BTreeMap::from([("4242_7".to_owned(), Some(bad))]))
                .is_err()
        );
        // Too many rows in a handcrafted file.
        std::fs::create_dir_all(fixture.dir()).expect("dir");
        std::fs::set_permissions(fixture.dir(), std::fs::Permissions::from_mode(0o700))
            .expect("mode");
        let rows = (0..MAX_STORE_ENTRIES + 1)
            .map(|index| {
                let win = format!("4242_{}", index + 1);
                let (pid, wid) = parse_window_id(&win).expect("win");
                serde_json::json!({
                    "win": win,
                    "wid": wid,
                    "pid": pid,
                    "launch_date_ns": 1,
                    "bundle_id": "com.example.app",
                    "data": canonical(None),
                })
            })
            .collect::<Vec<_>>();
        let file = serde_json::json!({
            "version": STORE_VERSION,
            "session_uid": fixture.store.uid,
            "session_asid": fixture.store.asid,
            "entries": rows,
        });
        std::fs::write(
            fixture.store.path(),
            serde_json::to_vec(&file).expect("encode"),
        )
        .expect("write overbound");
        std::fs::set_permissions(fixture.store.path(), std::fs::Permissions::from_mode(0o600))
            .expect("mode");
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert!(reopened.entries().is_empty());
        assert!(
            reopened
                .diagnostic()
                .is_some_and(|text| text.contains("row bound")),
            "{:?}",
            reopened.diagnostic()
        );
        // Oversized whole file.
        std::fs::write(fixture.store.path(), vec![b' '; MAX_STORE_FILE_BYTES + 1])
            .expect("write big");
        std::fs::set_permissions(fixture.store.path(), std::fs::Permissions::from_mode(0o600))
            .expect("mode");
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert!(reopened.entries().is_empty());
        assert!(reopened.diagnostic().is_some());
    }

    #[test]
    fn cleared_row_removes_file_entry_and_keeps_null_memory() {
        let mut fixture = TestStore::new();
        let data = canonical(Some(true));
        fixture
            .store
            .apply_changes(BTreeMap::from([(
                "4242_7".to_owned(),
                Some(entry("4242_7", &data)),
            )]))
            .expect("persist");
        let (_, mtime) = snapshot(&fixture);
        fixture
            .store
            .apply_changes(BTreeMap::from([("4242_7".to_owned(), None)]))
            .expect("clear");
        assert!(fixture.store.entries().get("4242_7").is_none());
        assert_eq!(fixture.store.durable().get("4242_7"), Some(&None));
        let (bytes, cleared_mtime) = snapshot(&fixture);
        assert_ne!(mtime, cleared_mtime, "clear rewrote the snapshot");
        assert!(!String::from_utf8_lossy(&bytes).contains("4242_7"));
        let reopened =
            IntentStore::open_at_unchecked(fixture.dir(), fixture.store.uid, fixture.store.asid);
        assert!(reopened.entries().is_empty());
        assert_eq!(reopened.diagnostic(), None);
    }

    #[test]
    fn launch_date_conversion_is_stable_and_fails_closed() {
        assert_eq!(
            launch_date_ns_from_bits(123.0f64.to_bits()),
            Some(123_000_000_000)
        );
        assert_eq!(launch_date_ns_from_bits(f64::NAN.to_bits()), None);
        assert_eq!(launch_date_ns_from_bits((-1.0f64).to_bits()), None);
        assert_eq!(launch_date_ns_from_bits(f64::INFINITY.to_bits()), None);
    }
}
