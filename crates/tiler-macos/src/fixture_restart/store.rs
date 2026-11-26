//! Isolated intent-store fingerprint: exact bytes plus metadata for every
//! file, the M3 persisted-float-row proof and the M4 unchanged-absence proof.
//! Read-only; never creates, repairs or deletes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::fixture_leg::FIXTURE_SCOPE;

// ---------------------------------------------------------------------------
// Isolated intent-store fingerprint (bytes plus metadata)
// ---------------------------------------------------------------------------

/// Maximum bytes read from any one store file (the store's own bound plus
/// one byte to detect an over-bound file).
const STORE_READ_BOUND: u64 = crate::intent_store::MAX_STORE_FILE_BYTES as u64;

/// Exact store-directory metadata retained for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreDirMeta {
    pub(crate) mode: u32,
    pub(crate) uid: u32,
    pub(crate) ino: u64,
    pub(crate) mtime_ns: i64,
}

/// Exact one-file metadata plus bytes retained for comparison. The bytes are
/// kept in memory for exact equality and never emitted in evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreFile {
    pub(crate) name: String,
    pub(crate) len: u64,
    pub(crate) mode: u32,
    pub(crate) uid: u32,
    pub(crate) ino: u64,
    pub(crate) mtime_ns: i64,
    pub(crate) bytes: Vec<u8>,
}

/// One isolated store fingerprint: absent, or an exact directory plus file
/// snapshot (names sorted). Equality covers every byte and every metadata
/// field retained here (mode, owner, inode, modification time, length), so a
/// same-bytes atomic rewrite still reports drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreSnapshot {
    pub(crate) present: bool,
    pub(crate) dir: Option<StoreDirMeta>,
    pub(crate) files: Vec<StoreFile>,
}

impl StoreSnapshot {
    pub(crate) fn absent() -> Self {
        Self {
            present: false,
            dir: None,
            files: Vec::new(),
        }
    }

    /// True when at least one intent-store row file is present.
    pub(crate) fn has_row(&self) -> bool {
        self.files
            .iter()
            .any(|file| file.name.starts_with("intent-v1-") && file.name.ends_with(".json"))
    }

    /// Metadata-only evidence: presence, entry counts, total bytes, exact
    /// mode/owner/timestamps, and a deterministic content digest. No file
    /// names (they carry the login namespace) and no raw bytes.
    pub(crate) fn evidence(&self) -> serde_json::Value {
        let total_bytes: u64 = self.files.iter().map(|file| file.len).sum();
        let rows = self
            .files
            .iter()
            .filter(|file| file.name.starts_with("intent-v1-") && file.name.ends_with(".json"))
            .count();
        serde_json::json!({
            "present": self.present,
            "files": self.files.len(),
            "rows": rows,
            "total_bytes": total_bytes,
            "dir_mode": self.dir.as_ref().map(|dir| format!("{:04o}", dir.mode)),
            "dir_uid": self.dir.as_ref().map(|dir| dir.uid),
            "dir_mtime_ns": self.dir.as_ref().map(|dir| dir.mtime_ns),
            "digest": format!("{:016x}", self.digest()),
        })
    }

    fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        if let Some(dir) = &self.dir {
            hash = fnv1a(hash, b"dir");
            hash = fnv1a(hash, &dir.mode.to_le_bytes());
            hash = fnv1a(hash, &dir.uid.to_le_bytes());
            hash = fnv1a(hash, &dir.ino.to_le_bytes());
            hash = fnv1a(hash, &dir.mtime_ns.to_le_bytes());
        }
        for file in &self.files {
            hash = fnv1a(hash, b"file");
            hash = fnv1a(hash, file.name.as_bytes());
            hash = fnv1a(hash, &file.len.to_le_bytes());
            hash = fnv1a(hash, &file.mode.to_le_bytes());
            hash = fnv1a(hash, &file.uid.to_le_bytes());
            hash = fnv1a(hash, &file.ino.to_le_bytes());
            hash = fnv1a(hash, &file.mtime_ns.to_le_bytes());
            hash = fnv1a(hash, &file.bytes);
        }
        hash
    }
}

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Exact hydration label for the isolated store: an absent store is a valid
/// absence (no float intent was ever written), never a hydrated row set.
pub(crate) fn store_hydration_state(snapshot: &StoreSnapshot) -> &'static str {
    if !snapshot.present {
        "unchanged-absence"
    } else if snapshot.has_row() {
        "unchanged-rows"
    } else {
        "unchanged-directory-only"
    }
}

fn intent_row_file(name: &str) -> bool {
    name.starts_with("intent-v1-") && name.ends_with(".json")
}

/// M4 has no durable float intent: the isolated store must hold no intent row
/// file at the first stop, and the exact fingerprint equality then preserves
/// that absence through hydration. A present row (or an unreadable layout) is
/// a false success, never evidence of restoration. Pure; unit-tested.
pub(crate) fn require_m4_absence(snapshot: &StoreSnapshot) -> Result<serde_json::Value, String> {
    let has_row = snapshot.has_row();
    if has_row {
        return Err(
            "restart m4: the isolated store holds an intent row at the first stop (the durable absence of a row is the expected state); refusing"
                .to_owned(),
        );
    }
    Ok(serde_json::json!({
        "has_row": has_row,
        "present": snapshot.present,
        "hydration_state": store_hydration_state(snapshot),
    }))
}

/// Exact M3 float-persistence verdict over the isolated store bytes captured
/// after the first stop. Every intended float window must have exactly one
/// persisted row bound to the fixture PID, its exact launch lifetime and
/// bundle, and a decoded `float:true` intent; no other row may claim
/// `float:true`. Rows that do not claim a float are ignored here (the
/// fingerprint guard still covers their bytes). Any malformed/ambiguous
/// row refuses rather than guessing a binding.
pub(crate) fn require_m3_float_rows(
    snapshot: &StoreSnapshot,
    float_windows: &[(String, u32)],
    fixture_pid: u32,
    fixture_binding: &crate::intent_store::LiveBinding,
) -> Result<serde_json::Value, String> {
    if fixture_binding.bundle_id != FIXTURE_SCOPE {
        return Err(
            "restart m3: live fixture binding is not the signed fixture bundle; refusing"
                .to_owned(),
        );
    }
    if !snapshot.present {
        return Err(
            "restart m3: the isolated store is absent after the first stop (float persistence unproven); refusing the second launch"
                .to_owned(),
        );
    }
    let expected: BTreeMap<String, u32> = float_windows
        .iter()
        .map(|(_, wid)| (crate::runtime_types::window_id(fixture_pid, *wid), *wid))
        .collect();
    let mut rows: BTreeMap<String, crate::intent_store::StoredEntry> = BTreeMap::new();
    for file in snapshot
        .files
        .iter()
        .filter(|file| intent_row_file(&file.name))
    {
        for entry in crate::intent_store::decode_store_entries(&file.bytes)
            .map_err(|error| format!("restart m3: persisted store row unreadable ({error})"))?
        {
            if rows.insert(entry.win.clone(), entry).is_some() {
                return Err(
                    "restart m3: the isolated store repeats a window id across rows; refusing"
                        .to_owned(),
                );
            }
        }
    }
    if rows.is_empty() {
        return Err(
            "restart m3: the isolated store has no persisted row after the first stop (float persistence unproven); refusing the second launch"
                .to_owned(),
        );
    }
    let mut observed_floats = BTreeSet::new();
    for entry in rows.values() {
        let intent = tiler_core::intent_data::decode_intent_data(&entry.data).map_err(|error| {
            format!(
                "restart m3: persisted row intent is unusable ({}); refusing",
                error.as_str()
            )
        })?;
        if intent.float != Some(true) {
            continue;
        }
        observed_floats.insert(entry.win.clone());
        let Some(expected_wid) = expected.get(&entry.win) else {
            return Err(
                "restart m3: the isolated store holds a float row for a window that was not an intended float; refusing"
                    .to_owned(),
            );
        };
        if entry.pid != fixture_pid
            || entry.wid != *expected_wid
            || entry.launch_date_ns != fixture_binding.launch_date_ns
            || entry.bundle_id != fixture_binding.bundle_id
        {
            return Err(
                "restart m3: a persisted float row is not bound to the exact fixture lifetime; refusing"
                    .to_owned(),
            );
        }
    }
    for win in expected.keys() {
        if !observed_floats.contains(win) {
            return Err(
                "restart m3: an intended float window has no persisted float row after the first stop; refusing"
                    .to_owned(),
            );
        }
    }
    if observed_floats.len() != expected.len() {
        return Err(
            "restart m3: persisted float rows do not match the intended float set; refusing"
                .to_owned(),
        );
    }
    Ok(serde_json::json!({
        "expected_floats": expected.len(),
        "observed_float_rows": observed_floats.len(),
        "exact_lifetime_bindings": true,
        "no_unexpected_float_rows": true,
    }))
}

/// Read one isolated store fingerprint from an exact directory. Absent is a
/// valid fingerprint (no persistence yet); a symlink, non-directory, nested
/// entry, non-regular file, over-bound file, or unreadable metadata/bytes
/// refuses as uncertainty. Read-only; never creates, repairs, or deletes.
pub(crate) fn read_store_snapshot(store_dir: &Path) -> Result<StoreSnapshot, String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir_meta = match std::fs::symlink_metadata(store_dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("restart store: isolated store is a symlink; refusing".to_owned());
        }
        Ok(meta) if meta.is_dir() => meta,
        Ok(_) => {
            return Err("restart store: isolated store is not a directory; refusing".to_owned());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StoreSnapshot::absent());
        }
        Err(error) => {
            return Err(format!(
                "restart store: isolated store metadata unreadable ({error}); refusing"
            ));
        }
    };
    let dir = StoreDirMeta {
        mode: dir_meta.permissions().mode(),
        uid: dir_meta.uid(),
        ino: dir_meta.ino(),
        mtime_ns: mtime_ns(&dir_meta),
    };
    let mut files = Vec::new();
    let entries = std::fs::read_dir(store_dir)
        .map_err(|error| format!("restart store: isolated store unreadable ({error}); refusing"))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("restart store: store entry unreadable ({error}); refusing")
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "restart store: store entry name is not UTF-8; refusing".to_owned())?;
        // Open exactly once and fingerprint the opened file: a path stat
        // followed by a separate read is a TOCTOU window, and an unopened
        // path can grow between the length check and the read. `O_NOFOLLOW`
        // plus post-open metadata judge the object actually read, while a
        // bounded read of `bound + 1` refuses any file that grew past the
        // store envelope.
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        let opened = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(entry.path())
        {
            Ok(file) => file,
            Err(error) if error.raw_os_error() == Some(libc::ELOOP) => {
                return Err(format!(
                    "restart store: store entry {name} is a symlink; refusing"
                ));
            }
            Err(error) => {
                return Err(format!(
                    "restart store: store entry {name} unreadable ({error}); refusing"
                ));
            }
        };
        let meta = opened.metadata().map_err(|error| {
            format!("restart store: store entry {name} metadata unreadable ({error}); refusing")
        })?;
        if !meta.is_file() {
            return Err(format!(
                "restart store: store entry {name} is not a regular file; refusing"
            ));
        }
        if meta.len() > STORE_READ_BOUND {
            return Err(format!(
                "restart store: store entry {name} exceeds the store bound; refusing"
            ));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
        opened
            .take(STORE_READ_BOUND + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                format!("restart store: store entry {name} bytes unreadable ({error}); refusing")
            })?;
        if bytes.len() as u64 > STORE_READ_BOUND {
            return Err(format!(
                "restart store: store entry {name} exceeds the store bound; refusing"
            ));
        }
        files.push(StoreFile {
            name,
            len: bytes.len() as u64,
            mode: meta.permissions().mode(),
            uid: meta.uid(),
            ino: meta.ino(),
            mtime_ns: mtime_ns(&meta),
            bytes,
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(StoreSnapshot {
        present: true,
        dir: Some(dir),
        files,
    })
}

fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    meta.mtime()
        .saturating_mul(1_000_000_000)
        .saturating_add(meta.mtime_nsec())
}
