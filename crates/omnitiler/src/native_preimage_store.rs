//! Adapter-owned native-state preimage slot: one versioned snapshot of
//! per-window coupled native recovery records.
//!
//! Separate from the core-owned opaque intent slot (`float_intent_store`):
//! this file never carries core intent bytes, and the opaque intent file
//! never carries native recovery detail. One record per live subject (`win`,
//! normalized like the KWin `normalizeNativeId` utility) carries nullable
//! per-property preimages; untouched state is never written, so every slot
//! is explicit nullable and the executor later arms only relevant slots:
//!
//! * `maximize_mode`: exact KWin `maximizeMode` axes (`0` restore,
//!   `1` vertical, `2` horizontal, `3` full), not a bool.
//! * `fullscreen`: exact `fullScreen` flag.
//! * `desktops`: observed desktop id memberships (`null` untouched,
//!   `[]` sticky/all-desktops exactly since a sticky window reads an empty
//!   list, nonempty the actual memberships for sticky-off restore).
//! * `stacking`: paired `keepAbove`/`keepBelow` flags as read. KWin couples
//!   them in the common path (`setKeepAbove(true)` clears `keepBelow` and
//!   vice versa, `src/window.cpp:592-627` at the pinned 6.8.1/6.6 revisions),
//!   but forced rules can hold both true, so the slot stores the raw pair
//!   without inventing exclusivity; both save and restore together.
//!
//! No rects, focus, geometry, layout, or core intent.
//!
//! Storage, namespace, and authorization reuse the float-intent primitives:
//! `<root>/omnitiler/native-preimage.json` under the same anchored
//! no-follow root/project gates, atomic replace, and temp discipline, with
//! the serving bus id plus current `org.kde.KWin` owner as namespace
//! (`IntentNamespace`, derived from the bus by the service layer, never from
//! the caller). A bus-id or owner mismatch reads usable empty exactly
//! like a missing file; the next full write atomically replaces the
//! stale file. Missing content reads plain empty; unreadable, corrupt (including any
//! non-`v:1` store body), or oversize content degrades with a fixed reason
//! and zero counts. Write failures retain local preimages; no retry
//! machinery. The server returns the stored snapshot only; joining it to
//! exact live UUIDs is the executor's proof, not this slot's. A KWin restart
//! invalidates the namespace so stale UUIDs cannot authorize restoration.
//!
//! Wire contract on `com.omnitiler.Planner1` (see planner_service):
//! `ReadNativePreimage({"v":1,"correlation_id":..})` returns `ok` with
//! `records`/`stored`/`returned`, or `degraded` empty with
//! `namespace-unavailable|unreadable|corrupt`, or
//! `rejected` with `unauthorized|not-kwin-owner`.
//! `WriteNativePreimage({"v":1,"correlation_id":..,"records":[{win,
//! maximize_mode,fullscreen,desktops,stacking}..]})` persists the full
//! preimage snapshot (empty `records` clears) and returns `stored`, or
//! `rejected` (`unauthorized|not-kwin-owner|invalid-request|
//! invalid-members`), or `unavailable` (`namespace-unavailable|
//! store-unavailable`). Oversize wire requests fail as `Unavailable`; an
//! oversized serialized snapshot rejects before replacing the file.
//! Logs carry only the validated correlation (or `-`), op, outcome, record
//! counts, and fixed reasons: never ids, owners, bus ids, paths, or payloads.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::float_intent_store::{
    IntentNamespace, MAX_STORE_FILE_BYTES, atomic_replace, create_project, normalize_member_id,
    open_project, open_root, read_final, temp_file_name,
};

/// D-Bus member names for the preimage routes, mirroring the explicit
/// `#[zbus(name = ...)]` attributes in the service (attributes require
/// literals, so tests pin the two spellings together here).
#[cfg(test)]
pub(crate) const READ_METHOD: &str = "ReadNativePreimage";
#[cfg(test)]
pub(crate) const WRITE_METHOD: &str = "WriteNativePreimage";
/// Record cap per snapshot, matching the opaque intent bound.
pub(crate) const MAX_PREIMAGE_RECORDS: usize = 10_000;
/// Desktop membership cap per record, matching the adapter's native decode
/// bound (`MAX_WINDOW_DESKTOPS`).
pub(crate) const MAX_PREIMAGE_DESKTOPS: usize = 4096;
/// Store schema version for the separate preimage file.
pub(crate) const PREIMAGE_STORE_VERSION: u32 = 1;
/// Separate slot filename: never the opaque intent file.
pub(crate) const PREIMAGE_STORE_FILE_NAME: &str = "native-preimage.json";
const PREIMAGE_TEMP_FILE_PREFIX: &str = ".native-preimage.json.tmp.";

/// Paired stacking preimage: both flags save and restore together.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StackingPreimage {
    pub(crate) above: bool,
    pub(crate) below: bool,
}

/// Coupled native recovery record for one verified live subject. Every slot
/// is explicit nullable: `null` means untouched (never written, never
/// restored) and the executor arms only relevant slots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativePreimage {
    pub(crate) win: String,
    pub(crate) maximize_mode: Option<u8>,
    pub(crate) fullscreen: Option<bool>,
    pub(crate) desktops: Option<Vec<String>>,
    pub(crate) stacking: Option<StackingPreimage>,
}

/// Record shape failure class: structural (wrong JSON types/keys) versus
/// semantic (well-typed but unusable content).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldError {
    Structural,
    Semantic,
}

/// Convert one raw record value with exact keys
/// (`win,maximize_mode,fullscreen,desktops,stacking`, all present) into the
/// typed record. Structural failures are wrong types/keys; semantic failures
/// are bad subjects, out-of-range modes, bad memberships, or all-null slots.
fn convert_record(value: &serde_json::Value) -> Result<NativePreimage, FieldError> {
    use FieldError::{Semantic, Structural};
    let object = value.as_object().ok_or(Structural)?;
    if object.len() != 5
        || !object.contains_key("win")
        || !object.contains_key("maximize_mode")
        || !object.contains_key("fullscreen")
        || !object.contains_key("desktops")
        || !object.contains_key("stacking")
    {
        return Err(Structural);
    }
    let win = object["win"].as_str().ok_or(Structural)?.to_owned();
    let maximize_mode = match &object["maximize_mode"] {
        serde_json::Value::Null => None,
        value => {
            let mode = value.as_u64().ok_or(Structural)?;
            if mode > u64::from(u8::MAX) {
                return Err(Structural);
            }
            Some(mode as u8)
        }
    };
    let fullscreen = match &object["fullscreen"] {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(flag) => Some(*flag),
        _ => return Err(Structural),
    };
    let desktops = match &object["desktops"] {
        serde_json::Value::Null => None,
        serde_json::Value::Array(members) => {
            let mut ids = Vec::with_capacity(members.len());
            for member in members {
                ids.push(member.as_str().ok_or(Structural)?.to_owned());
            }
            Some(ids)
        }
        _ => return Err(Structural),
    };
    let stacking = match &object["stacking"] {
        serde_json::Value::Null => None,
        serde_json::Value::Object(flags) => {
            if flags.len() != 2 || !flags.contains_key("above") || !flags.contains_key("below") {
                return Err(Structural);
            }
            let (Some(above), Some(below)) = (flags["above"].as_bool(), flags["below"].as_bool())
            else {
                return Err(Structural);
            };
            Some(StackingPreimage { above, below })
        }
        _ => return Err(Structural),
    };
    check_record(&NativePreimage {
        win,
        maximize_mode,
        fullscreen,
        desktops,
        stacking,
    })
    .map_err(|_| Semantic)
}

/// Semantic record check shared by the wire/file converters and the store
/// write: normalize subjects and memberships, reject out-of-range modes,
/// duplicate or over-count memberships, and all-null slots. Stacking passes
/// through exactly as read: no native exclusivity is invented. Structural
/// shape is the caller's job.
fn check_record(record: &NativePreimage) -> Result<NativePreimage, FieldError> {
    use FieldError::Semantic;
    let win = normalize_member_id(&record.win).ok_or(Semantic)?;
    if record.maximize_mode.is_some_and(|mode| mode > 3) {
        return Err(Semantic);
    }
    let desktops = match &record.desktops {
        None => None,
        Some(members) => {
            if members.len() > MAX_PREIMAGE_DESKTOPS {
                return Err(Semantic);
            }
            let mut seen = HashSet::with_capacity(members.len());
            let mut ids = Vec::with_capacity(members.len());
            for member in members {
                let id = normalize_member_id(member).ok_or(Semantic)?;
                if !seen.insert(id.clone()) {
                    return Err(Semantic);
                }
                ids.push(id);
            }
            Some(ids)
        }
    };
    // Both-true stacking is a genuine native read under forced rules: stored
    // as-is, never rejected. Storing facts grants no acquisition permission;
    // core preowned flags already prevent acquisition.
    if record.maximize_mode.is_none()
        && record.fullscreen.is_none()
        && record.desktops.is_none()
        && record.stacking.is_none()
    {
        return Err(Semantic);
    }
    Ok(NativePreimage {
        win,
        maximize_mode: record.maximize_mode,
        fullscreen: record.fullscreen,
        desktops,
        stacking: record.stacking.clone(),
    })
}

fn validate_preimages(records: &[NativePreimage]) -> Result<Vec<NativePreimage>, &'static str> {
    if records.len() > MAX_PREIMAGE_RECORDS {
        return Err("invalid-members");
    }
    let mut seen = HashSet::with_capacity(records.len());
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        if !seen.insert(record.win.clone()) {
            return Err("invalid-members");
        }
        out.push(record.clone());
    }
    Ok(out)
}

/// Private runtime store rooted at `root`. Stateless: every call re-opens
/// and re-gates the anchored fds. Same root as the opaque intent slot, own
/// file.
#[derive(Clone, Debug)]
pub(crate) struct NativePreimageStore {
    root: PathBuf,
}

impl NativePreimageStore {
    /// Production root from `XDG_RUNTIME_DIR`; `None` when unset or empty.
    pub(crate) fn from_runtime_dir() -> Option<Self> {
        let dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
        if dir.is_empty() {
            return None;
        }
        Some(Self::with_root(PathBuf::from(dir)))
    }

    /// Test seam: store rooted at a private directory.
    pub(crate) fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn read_preimages(&self, namespace: &IntentNamespace) -> PreimageRead {
        let Ok(root) = open_root(&self.root) else {
            return PreimageRead::degraded("unreadable");
        };
        let project = match open_project(&root) {
            Ok(Some(fd)) => fd,
            Ok(None) => return PreimageRead::empty(),
            Err(()) => return PreimageRead::degraded("unreadable"),
        };
        let bytes = match read_final(&project, PREIMAGE_STORE_FILE_NAME) {
            Ok(None) => return PreimageRead::empty(),
            Ok(Some(bytes)) => bytes,
            Err(()) => return PreimageRead::degraded("unreadable"),
        };
        let records = match parse_store_bytes(&bytes, namespace) {
            Ok(records) => records,
            Err(reason) => return PreimageRead::degraded(reason),
        };
        let stored = records.len();
        PreimageRead::ok(records, stored)
    }

    /// Replace with the full preimage snapshot (empty clears). Subjects
    /// normalize; duplicates, over-count, out-of-range modes, bad desktop
    /// memberships, both-true stacking, and all-null slots reject before
    /// touching disk. Filesystem failures retain local preimages.
    pub(crate) fn write_preimages(
        &self,
        namespace: &IntentNamespace,
        records: &[NativePreimage],
    ) -> Result<usize, &'static str> {
        let mut checked = Vec::with_capacity(records.len());
        for record in records {
            checked.push(check_record(record).map_err(|_| "invalid-members")?);
        }
        let normalized = validate_preimages(&checked)?;
        let body = serde_json::to_string(&PreimageStoreWrite {
            v: PREIMAGE_STORE_VERSION,
            bus: namespace.bus_id(),
            kwin: namespace.kwin_owner(),
            records: &normalized,
        })
        .map_err(|_| "store-unavailable")?;
        if body.len() as u64 > MAX_STORE_FILE_BYTES {
            return Err("invalid-members");
        }
        let Ok(root) = open_root(&self.root) else {
            return Err("store-unavailable");
        };
        let project = match open_project(&root) {
            Ok(Some(fd)) => fd,
            Ok(None) => create_project(&root).map_err(|()| "store-unavailable")?,
            Err(()) => return Err("store-unavailable"),
        };
        // Refuse to replace a non-conforming file: only a missing file or
        // our own `0600` regular file may be replaced, so a write never
        // follows a link to foreign data.
        if crate::float_intent_store::gate_final(&project, PREIMAGE_STORE_FILE_NAME).is_err() {
            return Err("store-unavailable");
        }
        atomic_replace(
            &project,
            body.as_bytes(),
            &temp_file_name(PREIMAGE_TEMP_FILE_PREFIX),
            PREIMAGE_STORE_FILE_NAME,
        )
        .map_err(|()| "store-unavailable")?;
        Ok(normalized.len())
    }
}

/// Read outcome: returned records and the readable on-disk count (`0` when
/// missing or degraded), plus the degraded reason if the content was unusable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreimageRead {
    pub(crate) records: Vec<NativePreimage>,
    pub(crate) stored: usize,
    pub(crate) degraded: Option<&'static str>,
}

impl PreimageRead {
    fn empty() -> Self {
        Self {
            records: Vec::new(),
            stored: 0,
            degraded: None,
        }
    }

    fn ok(records: Vec<NativePreimage>, stored: usize) -> Self {
        Self {
            records,
            stored,
            degraded: None,
        }
    }

    fn degraded(reason: &'static str) -> Self {
        Self {
            records: Vec::new(),
            stored: 0,
            degraded: Some(reason),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreimageStoreFile {
    v: u32,
    bus: String,
    kwin: String,
    records: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct PreimageStoreWrite<'a> {
    v: u32,
    bus: &'a str,
    kwin: &'a str,
    records: &'a [NativePreimage],
}

fn parse_store_bytes(
    bytes: &[u8],
    namespace: &IntentNamespace,
) -> Result<Vec<NativePreimage>, &'static str> {
    let file: PreimageStoreFile = serde_json::from_slice(bytes).map_err(|_| "corrupt")?;
    if file.v != PREIMAGE_STORE_VERSION {
        return Err("corrupt");
    }
    if file.bus != namespace.bus_id() || file.kwin != namespace.kwin_owner() {
        return Ok(Vec::new());
    }
    let mut records = Vec::with_capacity(file.records.len());
    for entry in &file.records {
        records.push(convert_record(entry).map_err(|_| "corrupt")?);
    }
    validate_preimages(&records).map_err(|_| "corrupt")
}

// ---------------------------------------------------------------------------
// Wire JSON: requests, replies.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteWire {
    v: u32,
    correlation_id: String,
    records: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadWire {
    v: u32,
    correlation_id: String,
}

#[derive(Debug)]
pub(crate) struct WritePreimageRequest {
    pub(crate) correlation: String,
    pub(crate) records: Vec<NativePreimage>,
}

#[derive(Debug)]
pub(crate) struct ReadPreimageRequest {
    pub(crate) correlation: String,
}

/// Owned parse failure: validated correlation (or `-`) plus a fixed reason.
#[derive(Debug)]
pub(crate) struct PreimageParseError {
    pub(crate) correlation: String,
    pub(crate) reason: &'static str,
}

impl PreimageParseError {
    fn new(correlation: &str, reason: &'static str) -> Self {
        Self {
            correlation: correlation.to_owned(),
            reason,
        }
    }
}

fn valid_wire(v: u32, correlation_id: &str) -> Result<String, PreimageParseError> {
    if v != PREIMAGE_STORE_VERSION || !crate::float_intent_store::valid_correlation(correlation_id)
    {
        return Err(PreimageParseError::new("-", "invalid-request"));
    }
    Ok(correlation_id.to_owned())
}

/// Structural failures are `invalid-request`; over-count, duplicate subjects,
/// un-normalizable subjects, out-of-range modes, bad desktop memberships,
/// and all-null slots are `invalid-members` (the store
/// write re-checks the same predicate before replacing).
pub(crate) fn parse_write_preimage_request(
    body: &str,
) -> Result<WritePreimageRequest, PreimageParseError> {
    let wire: WriteWire =
        serde_json::from_str(body).map_err(|_| PreimageParseError::new("-", "invalid-request"))?;
    let correlation = valid_wire(wire.v, &wire.correlation_id)?;
    if wire.records.len() > MAX_PREIMAGE_RECORDS {
        return Err(PreimageParseError::new(&correlation, "invalid-members"));
    }
    let mut records = Vec::with_capacity(wire.records.len());
    for entry in &wire.records {
        match convert_record(entry) {
            Ok(record) => records.push(record),
            Err(FieldError::Structural) => {
                return Err(PreimageParseError::new(&correlation, "invalid-request"));
            }
            Err(FieldError::Semantic) => {
                return Err(PreimageParseError::new(&correlation, "invalid-members"));
            }
        }
    }
    // Fail before disk on duplicate subjects; the store re-checks the same
    // predicate before replacing.
    validate_preimages(&records).map_err(|reason| PreimageParseError::new(&correlation, reason))?;
    Ok(WritePreimageRequest {
        correlation,
        records,
    })
}

/// Closed read shape: `{"v":1,"correlation_id":..}` only. Any extra key
/// rejects as `invalid-request`.
pub(crate) fn parse_read_preimage_request(
    body: &str,
) -> Result<ReadPreimageRequest, PreimageParseError> {
    let wire: ReadWire =
        serde_json::from_str(body).map_err(|_| PreimageParseError::new("-", "invalid-request"))?;
    let correlation = valid_wire(wire.v, &wire.correlation_id)?;
    Ok(ReadPreimageRequest { correlation })
}

/// Fixed-shape in-band rejection. Inputs must be validated correlation (or
/// `-`) and a fixed reason.
pub(crate) fn preimage_rejection(correlation: &str, reason: &str) -> String {
    serde_json::json!({
        "v": PREIMAGE_STORE_VERSION,
        "correlation_id": crate::float_intent_store::sanitize_correlation(correlation),
        "outcome": "rejected",
        "records": [],
        "reason": reason,
    })
    .to_string()
}

pub(crate) fn preimage_stored_reply(correlation: &str, stored: usize) -> String {
    serde_json::json!({
        "v": PREIMAGE_STORE_VERSION,
        "correlation_id": crate::float_intent_store::sanitize_correlation(correlation),
        "outcome": "stored",
        "stored": stored,
    })
    .to_string()
}

/// `outcome` is `rejected` or `unavailable`; the reason must be a fixed label.
pub(crate) fn preimage_failed_reply(correlation: &str, outcome: &str, reason: &str) -> String {
    serde_json::json!({
        "v": PREIMAGE_STORE_VERSION,
        "correlation_id": crate::float_intent_store::sanitize_correlation(correlation),
        "outcome": outcome,
        "reason": reason,
    })
    .to_string()
}

pub(crate) fn preimage_ok_reply(
    correlation: &str,
    records: &[NativePreimage],
    stored: usize,
) -> String {
    serde_json::json!({
        "v": PREIMAGE_STORE_VERSION,
        "correlation_id": crate::float_intent_store::sanitize_correlation(correlation),
        "outcome": "ok",
        "records": records,
        "stored": stored,
        "returned": records.len(),
    })
    .to_string()
}

/// Degraded reply: empty records with a fixed reason and zero counts.
pub(crate) fn preimage_degraded_reply(correlation: &str, reason: &str) -> String {
    serde_json::json!({
        "v": PREIMAGE_STORE_VERSION,
        "correlation_id": crate::float_intent_store::sanitize_correlation(correlation),
        "outcome": "degraded",
        "records": [],
        "stored": 0,
        "returned": 0,
        "reason": reason,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float_intent_store::{IntentNamespace, STORE_DIR_NAME};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_root(name: &str) -> PathBuf {
        let id = TEST_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("pat-preimage-{name}-{}-{id}", std::process::id()));
        let mut builder = std::fs::DirBuilder::new();
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&root).expect("temp root");
        root
    }

    fn drop_root(root: &Path) {
        let _ = std::fs::remove_dir_all(root);
    }

    fn namespace(bus: &str, owner: &str) -> IntentNamespace {
        IntentNamespace::parse(bus, owner).expect("valid namespace")
    }

    fn preimage(win: &str) -> NativePreimage {
        NativePreimage {
            win: win.to_owned(),
            maximize_mode: Some(3),
            fullscreen: Some(false),
            desktops: Some(Vec::new()),
            stacking: Some(StackingPreimage {
                above: true,
                below: false,
            }),
        }
    }

    fn project_fd_for(root: &Path) -> rustix::fd::OwnedFd {
        let root_fd = open_root(root).expect("root opens");
        match open_project(&root_fd).expect("project opens or missing") {
            Some(fd) => fd,
            None => create_project(&root_fd).expect("project created"),
        }
    }

    /// Raw file plant for corruption fixtures: test setup only, never the
    /// production path (which only writes through `atomic_replace`).
    fn plant_file(root: &Path, body: &[u8]) {
        use rustix::fs::{Mode, OFlags};
        let project = project_fd_for(root);
        let fd = rustix::fs::openat(
            &project,
            PREIMAGE_STORE_FILE_NAME,
            OFlags::WRONLY
                .union(OFlags::CREATE)
                .union(OFlags::TRUNC)
                .union(OFlags::CLOEXEC),
            Mode::from_bits_truncate(0o600),
        )
        .expect("plant opens");
        let mut file = std::fs::File::from(fd);
        file.write_all(body).expect("plant writes");
    }

    #[test]
    fn roundtrip_preserves_coupled_properties_exact_bytes() {
        let root = fresh_root("roundtrip");
        let store = NativePreimageStore::with_root(root.clone());
        let ns = namespace("bus-preimage", ":1.7");
        // Mixed partial records ride one snapshot: untouched slots stay
        // null, and `[]` desktops (sticky) differs from null (untouched).
        let records = vec![
            NativePreimage {
                win: "win-full".to_owned(),
                maximize_mode: Some(3),
                fullscreen: Some(true),
                desktops: Some(vec!["desk-1".to_owned()]),
                stacking: Some(StackingPreimage {
                    above: true,
                    below: false,
                }),
            },
            NativePreimage {
                win: "{12345678-1234-1234-1234-1234567890ab}".to_owned(),
                maximize_mode: Some(1),
                fullscreen: None,
                desktops: Some(Vec::new()),
                stacking: None,
            },
            NativePreimage {
                win: "win-below".to_owned(),
                maximize_mode: None,
                fullscreen: None,
                desktops: None,
                stacking: Some(StackingPreimage {
                    above: false,
                    below: true,
                }),
            },
            // Both-true stacking is a genuine native read under forced
            // rules: stored as-is with no acquisition permission implied.
            NativePreimage {
                win: "win-both".to_owned(),
                maximize_mode: None,
                fullscreen: None,
                desktops: None,
                stacking: Some(StackingPreimage {
                    above: true,
                    below: true,
                }),
            },
        ];
        assert_eq!(store.write_preimages(&ns, &records), Ok(4));
        // A new store instance reads the same content; the braced live UUID
        // normalizes to the bare form, byte-for-byte otherwise.
        let read = NativePreimageStore::with_root(root.clone()).read_preimages(&ns);
        assert_eq!(read.degraded, None);
        assert_eq!(read.stored, 4);
        assert_eq!(
            read.records,
            vec![
                records[0].clone(),
                NativePreimage {
                    win: "12345678-1234-1234-1234-1234567890ab".to_owned(),
                    ..records[1].clone()
                },
                records[2].clone(),
                records[3].clone(),
            ]
        );
        // Null desktops (untouched) stays distinguishable from empty
        // (sticky/all-desktops) after the roundtrip.
        assert_eq!(read.records[1].desktops, Some(Vec::new()));
        assert_eq!(read.records[2].desktops, None);
        drop_root(&root);
    }

    #[test]
    fn missing_store_reads_plain_empty() {
        let root = fresh_root("missing");
        let read = NativePreimageStore::with_root(root.clone())
            .read_preimages(&namespace("bus-m", ":1.7"));
        assert_eq!(read.degraded, None);
        assert_eq!(read.stored, 0);
        assert!(read.records.is_empty());
        drop_root(&root);
    }

    #[test]
    fn namespace_mismatch_reads_usable_empty_and_write_replaces() {
        let root = fresh_root("ns");
        let store = NativePreimageStore::with_root(root.clone());
        let first = namespace("bus-one", ":1.7");
        let bus_ns = namespace("bus-two", ":1.7");
        let owner_ns = namespace("bus-two", ":1.9");
        store
            .write_preimages(&first, &[preimage("win-a")])
            .expect("write");
        // Bus-id mismatch is a usable empty seed exactly like a missing
        // file: stale records never authorize restoration.
        let read = store.read_preimages(&bus_ns);
        assert_eq!(read.degraded, None);
        assert!(read.records.is_empty());
        assert_eq!(read.stored, 0);
        // First persist under the new bus atomically replaces the stale
        // file: the old subject never resurfaces.
        assert_eq!(store.write_preimages(&bus_ns, &[preimage("win-b")]), Ok(1));
        assert_eq!(
            store.read_preimages(&bus_ns).records,
            vec![preimage("win-b")]
        );
        let reread = store.read_preimages(&first);
        assert_eq!(reread.degraded, None);
        assert!(reread.records.is_empty());
        // Owner-only mismatch (same bus, new KWin owner) is likewise usable
        // empty, and its first persist replaces too.
        let read = store.read_preimages(&owner_ns);
        assert_eq!(read.degraded, None);
        assert!(read.records.is_empty());
        assert_eq!(read.stored, 0);
        assert_eq!(
            store.write_preimages(&owner_ns, &[preimage("win-c")]),
            Ok(1)
        );
        assert_eq!(
            store.read_preimages(&owner_ns).records,
            vec![preimage("win-c")]
        );
        drop_root(&root);
    }

    #[test]
    fn corrupt_and_oversize_and_bad_entries_degrade() {
        let root = fresh_root("corrupt");
        let store = NativePreimageStore::with_root(root.clone());
        let ns = namespace("bus-corrupt", ":1.7");
        let bad = [
            "not json{{{".to_owned(),
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":{}}"#.to_owned(),
            // Out-of-range maximize axes are corrupt on disk.
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"a","maximize_mode":4,"fullscreen":null,"desktops":null,"stacking":null}]}"#.to_owned(),
            // Duplicate subjects are corrupt on disk.
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"a","maximize_mode":0,"fullscreen":null,"desktops":null,"stacking":null},{"win":"a","maximize_mode":0,"fullscreen":null,"desktops":null,"stacking":null}]}"#.to_owned(),
            // Duplicate desktop memberships are corrupt on disk.
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"a","maximize_mode":null,"fullscreen":null,"desktops":["d1","d1"],"stacking":null}]}"#.to_owned(),
            // All-null slots are corrupt on disk.
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"a","maximize_mode":null,"fullscreen":null,"desktops":null,"stacking":null}]}"#.to_owned(),
            // A missing slot key is corrupt on disk (slots are explicit).
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"a","maximize_mode":null,"fullscreen":null,"desktops":null}]}"#.to_owned(),
            // An unknown record key is corrupt on disk.
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"a","maximize_mode":null,"fullscreen":null,"desktops":null,"stacking":null,"sticky":false}]}"#.to_owned(),
            // An unknown file key is corrupt on disk.
            r#"{"v":1,"bus":"bus-corrupt","kwin":":1.7","records":[],"sticky":false}"#.to_owned(),
            // Wrong store version is corrupt here (the opaque intent file is
            // a different slot with its own version).
            r#"{"v":2,"bus":"bus-corrupt","kwin":":1.7","records":[]}"#.to_owned(),
        ];
        for body in &bad {
            plant_file(&root, body.as_bytes());
            assert_eq!(
                store.read_preimages(&ns).degraded,
                Some("corrupt"),
                "{body}"
            );
        }
        plant_file(&root, &vec![b'x'; MAX_STORE_FILE_BYTES as usize + 1]);
        assert_eq!(store.read_preimages(&ns).degraded, Some("unreadable"));
        assert_eq!(store.write_preimages(&ns, &[preimage("win-a")]), Ok(1));
        assert_eq!(store.read_preimages(&ns).records, vec![preimage("win-a")]);
        drop_root(&root);
    }

    #[test]
    fn write_validates_before_touching_disk() {
        let root = fresh_root("validate");
        let store = NativePreimageStore::with_root(root.clone());
        let ns = namespace("bus-validate", ":1.7");
        assert_eq!(
            store.write_preimages(&ns, &[preimage("bad id")]),
            Err("invalid-members")
        );
        assert_eq!(
            store.write_preimages(&ns, &[preimage("a"), preimage("a")]),
            Err("invalid-members")
        );
        assert_eq!(
            store.write_preimages(
                &ns,
                &[NativePreimage {
                    maximize_mode: Some(4),
                    ..preimage("a")
                }],
            ),
            Err("invalid-members")
        );
        assert_eq!(
            store.write_preimages(
                &ns,
                &[NativePreimage {
                    desktops: Some(vec!["d1".to_owned(), "d1".to_owned()]),
                    ..preimage("a")
                }],
            ),
            Err("invalid-members")
        );
        // All-null slots reject before disk.
        assert_eq!(
            store.write_preimages(
                &ns,
                &[NativePreimage {
                    win: "a".to_owned(),
                    maximize_mode: None,
                    fullscreen: None,
                    desktops: None,
                    stacking: None,
                }],
            ),
            Err("invalid-members")
        );
        assert_eq!(
            store.write_preimages(&ns, &vec![preimage("a"); MAX_PREIMAGE_RECORDS + 1],),
            Err("invalid-members")
        );
        let read = store.read_preimages(&ns);
        assert_eq!(read.degraded, None);
        assert!(read.records.is_empty());
        drop_root(&root);
    }

    #[test]
    fn symlinked_slot_refuses_and_foreign_data_survives() {
        let root = fresh_root("symlink");
        let store = NativePreimageStore::with_root(root.clone());
        let ns = namespace("bus-symlink", ":1.7");
        let foreign = root.join("foreign.json");
        let sentinel = br#"{"v":1,"bus":"bus-symlink","kwin":":1.7","records":[]}"#;
        std::fs::write(&foreign, sentinel).expect("foreign");
        let _ = project_fd_for(&root);
        std::os::unix::fs::symlink(
            &foreign,
            root.join(format!("{STORE_DIR_NAME}/{PREIMAGE_STORE_FILE_NAME}")),
        )
        .expect("slot link");
        let read = store.read_preimages(&ns);
        assert_eq!(read.degraded, Some("unreadable"));
        assert!(read.records.is_empty());
        assert_eq!(
            store.write_preimages(&ns, &[preimage("win-a")]),
            Err("store-unavailable")
        );
        assert_eq!(std::fs::read(&foreign).expect("foreign"), sentinel);
        drop_root(&root);
    }

    #[test]
    fn wrong_mode_project_dir_refuses_without_chmod() {
        use std::os::unix::fs::PermissionsExt;
        let root = fresh_root("dirperm");
        let store = NativePreimageStore::with_root(root.clone());
        let ns = namespace("bus-dirperm", ":1.7");
        store
            .write_preimages(&ns, &[preimage("win-a")])
            .expect("write");
        let project = root.join(STORE_DIR_NAME);
        std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o755)).expect("loosen");
        assert_eq!(store.read_preimages(&ns).degraded, Some("unreadable"));
        assert_eq!(
            store.write_preimages(&ns, &[preimage("win-b")]),
            Err("store-unavailable")
        );
        assert_eq!(
            std::fs::symlink_metadata(&project)
                .expect("meta")
                .permissions()
                .mode()
                & 0o777,
            0o755,
            "existing dir must never be chmodded"
        );
        drop_root(&root);
    }

    #[test]
    fn separate_slot_leaves_opaque_intent_file_untouched() {
        let root = fresh_root("separate");
        let ns = namespace("bus-separate", ":1.7");
        let intent = crate::float_intent_store::FloatIntentStore::with_root(root.clone());
        intent
            .write_records(
                &ns,
                &[crate::float_intent_store::IntentRecord {
                    win: "win-a".to_owned(),
                    data: "x".to_owned(),
                }],
            )
            .expect("intent write");
        let store = NativePreimageStore::with_root(root.clone());
        store
            .write_preimages(&ns, &[preimage("win-a")])
            .expect("write");
        // Both slots coexist under one root with their own files.
        assert!(
            root.join(STORE_DIR_NAME)
                .join("float-intent.json")
                .is_file()
        );
        assert!(
            root.join(STORE_DIR_NAME)
                .join(PREIMAGE_STORE_FILE_NAME)
                .is_file()
        );
        assert_eq!(intent.read_membership(&ns).records.len(), 1);
        assert_eq!(store.read_preimages(&ns).records, vec![preimage("win-a")]);
        drop_root(&root);
    }

    #[test]
    fn wire_replies_are_bounded_fixed_shapes() {
        use crate::float_intent_store::{INTENT_MAX_REPLY_BYTES, sanitize_correlation};
        let ok = preimage_stored_reply("corr-1", 2);
        let parsed: serde_json::Value = serde_json::from_str(&ok).expect("valid JSON");
        assert_eq!(parsed["v"], 1);
        assert_eq!(parsed["outcome"], "stored");
        assert_eq!(parsed["stored"], 2);
        assert!(ok.len() <= INTENT_MAX_REPLY_BYTES);
        let rejected = preimage_rejection("evil corr", "unauthorized");
        assert!(rejected.contains("\"correlation_id\":\"-\""), "{rejected}");
        assert!(!rejected.contains("evil"), "{rejected}");
        assert_eq!(sanitize_correlation("ok-1"), "ok-1");
        let degraded = preimage_degraded_reply("corr-1", "corrupt");
        let parsed: serde_json::Value = serde_json::from_str(&degraded).expect("valid JSON");
        assert_eq!(parsed["v"], 1);
        assert_eq!(parsed["outcome"], "degraded");
        assert_eq!(parsed["records"].as_array().expect("array").len(), 0);
        assert_eq!(parsed["stored"], 0);
        assert_eq!(parsed["returned"], 0);
        let ok = preimage_ok_reply("corr-1", &[preimage("win-a")], 1);
        let parsed: serde_json::Value = serde_json::from_str(&ok).expect("valid JSON");
        assert_eq!(parsed["v"], 1);
        assert_eq!(parsed["stored"], 1);
        assert_eq!(parsed["returned"], 1);
        assert_eq!(parsed["records"].as_array().expect("array").len(), 1);
        assert_eq!(parsed["records"][0]["maximize_mode"], 3);
    }

    #[test]
    fn write_request_parsing_separates_structural_from_semantic() {
        let good = parse_write_preimage_request(
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":["d1"],"stacking":{"above":true,"below":false}}]}"#,
        )
        .expect("valid");
        assert_eq!(good.correlation, "corr-1");
        assert_eq!(
            good.records,
            vec![NativePreimage {
                win: "win-a".to_owned(),
                maximize_mode: Some(3),
                fullscreen: Some(true),
                desktops: Some(vec!["d1".to_owned()]),
                stacking: Some(StackingPreimage {
                    above: true,
                    below: false,
                }),
            }]
        );
        // Partial records with explicit nulls are valid: untouched slots
        // stay distinguishable from set ones.
        let partial = parse_write_preimage_request(
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":null,"fullscreen":false,"desktops":null,"stacking":null}]}"#,
        )
        .expect("partial valid");
        assert_eq!(
            partial.records,
            vec![NativePreimage {
                win: "win-a".to_owned(),
                maximize_mode: None,
                fullscreen: Some(false),
                desktops: None,
                stacking: None,
            }]
        );
        // Both-true stacking parses as ordinary raw facts.
        let both = parse_write_preimage_request(
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":null,"fullscreen":null,"desktops":null,"stacking":{"above":true,"below":true}}]}"#,
        )
        .expect("both-true valid");
        assert_eq!(
            both.records,
            vec![NativePreimage {
                win: "win-a".to_owned(),
                maximize_mode: None,
                fullscreen: None,
                desktops: None,
                stacking: Some(StackingPreimage {
                    above: true,
                    below: true,
                }),
            }]
        );
        // Structural failures surface as invalid-request.
        for body in [
            "not json",
            r#"{"v":1,"records":[]}"#,
            r#"{"v":2,"correlation_id":"corr-1","records":[]}"#,
            r#"{"v":1,"correlation_id":"bad corr","records":[]}"#,
            r#"{"v":1,"correlation_id":"corr-1"}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":{}}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a"}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":[],"stacking":null,"sticky":false}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":[]}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":"3","fullscreen":true,"desktops":null,"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3.5,"fullscreen":true,"desktops":null,"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":"yes","desktops":null,"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":{},"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":[42],"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":null,"stacking":{"above":true}}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":null,"stacking":{"above":true,"below":false,"extra":1}}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"win-a","maximize_mode":3,"fullscreen":true,"desktops":null,"stacking":{"above":"yes","below":false}}]}"#,
        ] {
            let error = parse_write_preimage_request(body).expect_err("must reject");
            assert_eq!(error.reason, "invalid-request", "{body}");
            assert!(!error.correlation.contains("bad"), "{body}");
        }
        let clear =
            parse_write_preimage_request(r#"{"v":1,"correlation_id":"corr-1","records":[]}"#)
                .expect("empty clears");
        assert!(clear.records.is_empty());
        // Semantic failures surface as invalid-members before disk.
        for body in [
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"bad id","maximize_mode":0,"fullscreen":null,"desktops":null,"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"a","maximize_mode":4,"fullscreen":null,"desktops":null,"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"a","maximize_mode":0,"fullscreen":null,"desktops":null,"stacking":null},{"win":"a","maximize_mode":0,"fullscreen":null,"desktops":null,"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"a","maximize_mode":null,"fullscreen":null,"desktops":["has space"],"stacking":null}]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[{"win":"a","maximize_mode":null,"fullscreen":null,"desktops":null,"stacking":null}]}"#,
        ] {
            let error = parse_write_preimage_request(body).expect_err("must reject");
            assert_eq!(error.reason, "invalid-members", "{body}");
        }
    }

    #[test]
    fn read_request_parsing_is_closed_v1_only() {
        let plain =
            parse_read_preimage_request(r#"{"v":1,"correlation_id":"corr-1"}"#).expect("valid");
        assert_eq!(plain.correlation, "corr-1");
        for body in [
            r#"{"v":1,"correlation_id":"corr-1","live":["win-a"]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[]}"#,
            r#"{"v":2,"correlation_id":"corr-1"}"#,
            r#"{"v":1,"correlation_id":"bad corr"}"#,
            r#"{"v":1}"#,
        ] {
            assert!(parse_read_preimage_request(body).is_err(), "{body}");
        }
    }
}
