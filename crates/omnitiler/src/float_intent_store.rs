//! Session-scoped opaque intent storage: one versioned snapshot of
//! `{win, data}` records.
//!
//! Rust-owned private store for settled opaque intent records: the core owns
//! the inner `data` bytes, the adapter stores them unchanged, and this layer
//! never interprets inner JSON or synthesizes floats. Each record binds the
//! storage-verified live subject (`win`, normalized like the KWin
//! `normalizeNativeId` utility) to the exact opaque core record (`data`,
//! `<=1024` UTF-8 bytes). No rects, focus, stacking, history, geometry, or
//! native writes. The narrow planner-service persistence exception for opaque
//! intent only.
//!
//! Layout: `<root>/omnitiler/float-intent.json` (`XDG_RUNTIME_DIR`
//! in production, a private temp root in tests). Every access is anchored:
//! the absolute root is opened component-by-component from `/`, each
//! ancestor with `O_DIRECTORY|O_NOFOLLOW`, so a symlink anywhere in the
//! chain refuses with `ELOOP` (only the final runtime dir is gated for
//! euid ownership and private mode; system/tmp parent modes are fine).
//! The project dir and final file open relative to that fd with
//! `O_NOFOLLOW`, and ownership (euid), type, and modes (project dir free
//! of group/other bits, file exactly `0600`) are checked with `fstat` on
//! the opened handles, never on paths. Final-file opens add `O_NONBLOCK`
//! so a planted FIFO can never block the gate (regular files are
//! unaffected) and reads degrade immediately. Nothing is ever chmodded:
//! a non-conforming directory or file refuses fail-closed. Writes replace
//! atomically (`O_CREAT|O_EXCL|O_NOFOLLOW` temp plus anchored `renameat`);
//! temp cleanup removes via anchored `unlinkat` only a temp file this exact
//! call created, so a pre-existing occupant is preserved byte-for-byte.
//! Every later access is dir-relative under the walked root fd, so a
//! swapped link can never redirect reads or writes to foreign data.
//!
//! Namespace: serving bus id plus current `org.kde.KWin` owner, derived from
//! the bus by the service layer, never from the caller. Mismatch degrades to
//! empty; the next full write replaces. Missing content reads plain empty;
//! unreadable, corrupt (including any non-`v:2` store body: the old `v:1`
//! membership format is unsupported with no conversion), or oversize content
//! degrades with a fixed reason and zero counts. Write failures retain local
//! intent; no retry machinery.
//!
//! Wire contract on `com.omnitiler.Planner1` (see planner_service):
//! `ReadFloatIntent({"v":2,"correlation_id":..})` returns `ok` with
//! `records`/`stored`/`returned`, or `degraded` empty with
//! `namespace-unavailable|unreadable|corrupt|namespace-mismatch`, or
//! `rejected` with `unauthorized|not-kwin-owner`.
//! `WriteFloatIntent({"v":2,"correlation_id":..,"records":[{win,data}..]})`
//! persists the full opaque snapshot (empty `records` clears) and returns
//! `stored`, or `rejected` (`unauthorized|not-kwin-owner|invalid-request|
//! invalid-members`), or `unavailable` (`namespace-unavailable|
//! store-unavailable`). Oversize bodies fail closed as `Unavailable` errors.
//! Logs carry only the validated correlation (or `-`), op, outcome, record
//! counts, and fixed reasons: never ids, owners, bus ids, paths, data, or
//! payloads.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fd::OwnedFd;
use rustix::fs::{AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};

/// D-Bus member names for the intent routes, mirroring the explicit
/// `#[zbus(name = ...)]` attributes in the service (attributes require
/// literals, so tests pin the two spellings together here).
#[cfg(test)]
pub(crate) const READ_METHOD: &str = "ReadFloatIntent";
#[cfg(test)]
pub(crate) const WRITE_METHOD: &str = "WriteFloatIntent";
/// Request cap: full 10000-record opaque snapshot bounded by the 4 MiB
/// envelope with margin for JSON framing.
pub(crate) const INTENT_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// Reply cap: full 10000-record read bounded by the 4 MiB envelope.
pub(crate) const INTENT_MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;
/// Record cap per snapshot, per world-contract intent arrays (<=10000).
pub(crate) const MAX_INTENT_RECORDS: usize = 10_000;
/// Opaque data bound per record: at most 1024 UTF-8 bytes (byte length).
pub(crate) const MAX_INTENT_DATA_BYTES: usize = 1024;
pub(crate) const MAX_CORRELATION_LEN: usize = 128;
pub(crate) const MAX_MEMBER_ID_LEN: usize = 128;
pub(crate) const MAX_NAMESPACE_FIELD_LEN: usize = 256;
pub(crate) const MAX_STORE_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const INTENT_STORE_VERSION: u32 = 2;
pub(crate) const STORE_DIR_NAME: &str = "omnitiler";
pub(crate) const STORE_FILE_NAME: &str = "float-intent.json";
const TEMP_FILE_PREFIX: &str = ".float-intent.json.tmp.";
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Validated namespace: serving bus id plus current KWin owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntentNamespace {
    bus_id: String,
    kwin_owner: String,
}

impl IntentNamespace {
    /// Both fields non-empty, bounded, and free of path separators,
    /// whitespace, and control bytes; the owner must parse as a unique name.
    pub(crate) fn parse(bus_id: &str, kwin_owner: &str) -> Option<Self> {
        if !valid_namespace_field(bus_id) || !valid_namespace_field(kwin_owner) {
            return None;
        }
        if zbus::names::UniqueName::try_from(kwin_owner).is_err() {
            return None;
        }
        Some(Self {
            bus_id: bus_id.to_owned(),
            kwin_owner: kwin_owner.to_owned(),
        })
    }

    /// Fail-closed sender check: sender equals the live-resolved owner and
    /// parses as a unique name.
    pub(crate) fn authorizes_sender(&self, sender: &str) -> bool {
        sender == self.kwin_owner && zbus::names::UniqueName::try_from(sender).is_ok()
    }

    /// Serving bus id for namespace binding in sibling slots.
    pub(crate) fn bus_id(&self) -> &str {
        &self.bus_id
    }

    /// Current KWin owner for namespace binding in sibling slots.
    pub(crate) fn kwin_owner(&self) -> &str {
        &self.kwin_owner
    }
}

fn valid_namespace_field(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_NAMESPACE_FIELD_LEN
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'{' | b'}')
        })
}

/// Non-empty bounded `[A-Za-z0-9-_.]` token (core correlation alphabet).
pub(crate) fn valid_correlation(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CORRELATION_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Log-safe echo: the validated token, else `-`.
pub(crate) fn sanitize_correlation(value: &str) -> &str {
    if valid_correlation(value) { value } else { "-" }
}

fn is_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MEMBER_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn is_uuid_text(text: &str) -> bool {
    const LENS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut parts = text.split('-');
    LENS.iter().all(|len| {
        parts.next().is_some_and(|part| {
            part.len() == *len && !part.is_empty() && part.bytes().all(|b| b.is_ascii_hexdigit())
        })
    }) && parts.next().is_none()
}

/// Normalize one live `internalId` like the KWin `normalizeNativeId`
/// utility: opaque passes through, one exact braced-UUID unwraps, else `None`.
pub(crate) fn normalize_member_id(value: &str) -> Option<String> {
    if is_opaque_id(value) {
        return Some(value.to_owned());
    }
    if value.len() == 38 && value.starts_with('{') && value.ends_with('}') {
        let inner = &value[1..37];
        if is_uuid_text(inner) && is_opaque_id(inner) {
            return Some(inner.to_owned());
        }
    }
    None
}

/// Opaque intent record: verified live subject plus exact core bytes.
/// The store never interprets `data`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct IntentRecord {
    pub(crate) win: String,
    pub(crate) data: String,
}

fn validate_records(records: &[IntentRecord]) -> Result<Vec<IntentRecord>, &'static str> {
    if records.len() > MAX_INTENT_RECORDS {
        return Err("invalid-members");
    }
    let mut seen = HashSet::with_capacity(records.len());
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let win = normalize_member_id(&record.win).ok_or("invalid-members")?;
        if record.data.len() > MAX_INTENT_DATA_BYTES {
            return Err("invalid-members");
        }
        if !seen.insert(win.clone()) {
            return Err("invalid-members");
        }
        out.push(IntentRecord {
            win,
            data: record.data.clone(),
        });
    }
    Ok(out)
}

/// Private runtime store rooted at `root`. Stateless: every call re-opens
/// and re-gates the anchored fds.
#[derive(Clone, Debug)]
pub(crate) struct FloatIntentStore {
    root: PathBuf,
}

impl FloatIntentStore {
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

    pub(crate) fn read_membership(&self, namespace: &IntentNamespace) -> IntentRead {
        let Ok(root) = open_root(&self.root) else {
            return IntentRead::degraded("unreadable");
        };
        let project = match open_project(&root) {
            Ok(Some(fd)) => fd,
            Ok(None) => return IntentRead::empty(),
            Err(()) => return IntentRead::degraded("unreadable"),
        };
        let bytes = match read_final(&project, STORE_FILE_NAME) {
            Ok(None) => return IntentRead::empty(),
            Ok(Some(bytes)) => bytes,
            Err(()) => return IntentRead::degraded("unreadable"),
        };
        let records = match parse_store_bytes(&bytes, namespace) {
            Ok(records) => records,
            Err(reason) => return IntentRead::degraded(reason),
        };
        let stored = records.len();
        IntentRead::ok(records, stored)
    }

    /// Replace with the full opaque snapshot (empty clears). Subjects
    /// normalize; duplicates, over-count, overbound data, and oversize bodies
    /// reject before touching disk. Inner `data` bytes are never interpreted.
    /// Filesystem failures retain local intent.
    pub(crate) fn write_records(
        &self,
        namespace: &IntentNamespace,
        records: &[IntentRecord],
    ) -> Result<usize, &'static str> {
        let normalized = validate_records(records)?;
        let body = serde_json::to_string(&StoreFile {
            v: INTENT_STORE_VERSION,
            bus: namespace.bus_id.clone(),
            kwin: namespace.kwin_owner.clone(),
            records: normalized.clone(),
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
        if gate_final(&project, STORE_FILE_NAME).is_err() {
            return Err("store-unavailable");
        }
        atomic_replace(
            &project,
            body.as_bytes(),
            &temp_file_name(TEMP_FILE_PREFIX),
            STORE_FILE_NAME,
        )
        .map_err(|()| "store-unavailable")?;
        Ok(normalized.len())
    }
}

/// Read outcome: returned records and the readable on-disk count (`0` when
/// missing or degraded), plus the degraded reason if the content was unusable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntentRead {
    pub(crate) records: Vec<IntentRecord>,
    pub(crate) stored: usize,
    pub(crate) degraded: Option<&'static str>,
}

impl IntentRead {
    fn empty() -> Self {
        Self {
            records: Vec::new(),
            stored: 0,
            degraded: None,
        }
    }

    fn ok(records: Vec<IntentRecord>, stored: usize) -> Self {
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

#[derive(Serialize, Deserialize)]
struct StoreFile {
    v: u32,
    bus: String,
    kwin: String,
    records: Vec<IntentRecord>,
}

fn parse_store_bytes(
    bytes: &[u8],
    namespace: &IntentNamespace,
) -> Result<Vec<IntentRecord>, &'static str> {
    let file: StoreFile = serde_json::from_slice(bytes).map_err(|_| "corrupt")?;
    if file.v != INTENT_STORE_VERSION {
        return Err("corrupt");
    }
    if file.bus != namespace.bus_id || file.kwin != namespace.kwin_owner {
        return Err("namespace-mismatch");
    }
    validate_records(&file.records).map_err(|_| "corrupt")
}

// ---------------------------------------------------------------------------
// Anchored filesystem: every access is fd-relative under a nofollow root.
// ---------------------------------------------------------------------------

const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const FILE_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);

fn not_found(error: &rustix::io::Errno) -> bool {
    std::io::Error::from(*error).kind() == std::io::ErrorKind::NotFound
}

fn euid() -> u32 {
    rustix::process::geteuid().as_raw()
}

/// Open the absolute root component-by-component from `/`, each ancestor
/// with `O_DIRECTORY|O_NOFOLLOW`: a symlink anywhere in the chain refuses
/// with `ELOOP`, and `..`/`.` components reject outright. Only the final
/// runtime dir is gated (euid-owned, no group/other bits); parent modes are
/// whatever the system or tmp hierarchy uses. Never chmodded. Shared by the
/// opaque intent slot and the separate native preimage slot (which passes
/// its own final filename below).
pub(crate) fn open_root(root: &Path) -> Result<OwnedFd, ()> {
    let mut components = root.components();
    if components.next() != Some(Component::RootDir) {
        return Err(());
    }
    let mut parent =
        rustix::fs::openat(rustix::fs::CWD, "/", DIR_FLAGS, Mode::empty()).map_err(|_| ())?;
    for component in components {
        let Component::Normal(name) = component else {
            return Err(());
        };
        parent = rustix::fs::openat(&parent, name, DIR_FLAGS, Mode::empty()).map_err(|_| ())?;
    }
    gate_private_dir(&parent)?;
    Ok(parent)
}

/// Dir gate on an opened handle: real dir, euid-owned, no group/other bits.
/// Shared with the native preimage slot.
pub(crate) fn gate_private_dir(fd: &OwnedFd) -> Result<(), ()> {
    let stat = rustix::fs::fstat(fd).map_err(|_| ())?;
    if stat.st_mode & S_IFMT != S_IFDIR || stat.st_uid != euid() || stat.st_mode & 0o077 != 0 {
        return Err(());
    }
    Ok(())
}

/// Open the project dir relative to the root fd. `Ok(None)` is missing.
/// Shared with the native preimage slot.
pub(crate) fn open_project(root: &OwnedFd) -> Result<Option<OwnedFd>, ()> {
    match rustix::fs::openat(root, STORE_DIR_NAME, DIR_FLAGS, Mode::empty()) {
        Ok(fd) => {
            gate_private_dir(&fd)?;
            Ok(Some(fd))
        }
        Err(error) if not_found(&error) => Ok(None),
        Err(_) => Err(()),
    }
}

/// Create the project dir (`mkdirat` never follows a same-name symlink;
/// `EEXIST` races re-open under the same nofollow gates) and verify the
/// opened handle. Never chmods. Shared with the native preimage slot.
pub(crate) fn create_project(root: &OwnedFd) -> Result<OwnedFd, ()> {
    match rustix::fs::mkdirat(root, STORE_DIR_NAME, Mode::from_bits_truncate(0o700)) {
        Ok(()) => {}
        Err(error) if not_found(&error) => return Err(()),
        Err(_) => {}
    }
    let fd = rustix::fs::openat(root, STORE_DIR_NAME, DIR_FLAGS, Mode::empty()).map_err(|_| ())?;
    gate_private_dir(&fd)?;
    Ok(fd)
}

/// Gate the final file on its opened nofollow handle: missing is `Ok(false)`,
/// our `0600` euid regular file is `Ok(true)`, anything else refuses.
/// Takes the slot filename so the separate native preimage slot reuses the
/// same gate on its own file.
pub(crate) fn gate_final(dir: &OwnedFd, file_name: &str) -> Result<bool, ()> {
    let fd = match rustix::fs::openat(dir, file_name, FILE_FLAGS, Mode::empty()) {
        Ok(fd) => fd,
        Err(error) if not_found(&error) => return Ok(false),
        Err(_) => return Err(()),
    };
    let stat = rustix::fs::fstat(&fd).map_err(|_| ())?;
    if stat.st_mode & S_IFMT != S_IFREG || stat.st_uid != euid() || stat.st_mode & 0o777 != 0o600 {
        return Err(());
    }
    Ok(true)
}

/// Bounded read through the gated handle: `take(cap + 1)` so a growing file
/// can never exceed the bound. Takes the slot filename for slot reuse.
pub(crate) fn read_final(dir: &OwnedFd, file_name: &str) -> Result<Option<Vec<u8>>, ()> {
    if !gate_final(dir, file_name)? {
        return Ok(None);
    }
    let fd = rustix::fs::openat(dir, file_name, FILE_FLAGS, Mode::empty()).map_err(|_| ())?;
    if gate_final_fd(&fd).is_err() {
        return Err(());
    }
    let mut bytes = Vec::new();
    std::fs::File::from(fd)
        .take(MAX_STORE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_STORE_FILE_BYTES {
        return Err(());
    }
    Ok(Some(bytes))
}

fn gate_final_fd(fd: &OwnedFd) -> Result<(), ()> {
    let stat = rustix::fs::fstat(fd).map_err(|_| ())?;
    if stat.st_mode & S_IFMT != S_IFREG || stat.st_uid != euid() || stat.st_mode & 0o777 != 0o600 {
        return Err(());
    }
    Ok(())
}

pub(crate) fn temp_file_name(prefix: &str) -> String {
    let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{}{id}", std::process::id())
}

/// Anchored atomic replace. Only a temp file this exact call created
/// (`O_CREAT|O_EXCL|O_NOFOLLOW`) is ever unlinked, via anchored `unlinkat`;
/// a pre-existing occupant fails creation and is preserved byte-for-byte.
/// The post-rename handle is re-gated before reporting success. Takes the
/// final slot filename so the separate native preimage slot reuses the same
/// replace on its own file.
pub(crate) fn atomic_replace(
    dir: &OwnedFd,
    body: &[u8],
    temp_name: &str,
    final_name: &str,
) -> Result<(), ()> {
    let fd = match rustix::fs::openat(
        dir,
        temp_name,
        OFlags::WRONLY
            .union(OFlags::CREATE)
            .union(OFlags::EXCL)
            .union(OFlags::NOFOLLOW)
            .union(OFlags::CLOEXEC),
        Mode::from_bits_truncate(0o600),
    ) {
        Ok(fd) => fd,
        Err(_) => return Err(()),
    };
    let wrote = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::from(fd);
        file.write_all(body)?;
        file.sync_all()?;
        Ok(())
    })();
    if wrote.is_err() {
        let _ = rustix::fs::unlinkat(dir, temp_name, AtFlags::empty());
        return Err(());
    }
    if rustix::fs::renameat(dir, temp_name, dir, final_name).is_err() {
        let _ = rustix::fs::unlinkat(dir, temp_name, AtFlags::empty());
        return Err(());
    }
    if gate_final(dir, final_name) != Ok(true) {
        return Err(());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Wire JSON: requests, replies, summaries.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RecordWire {
    win: serde_json::Value,
    data: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteWire {
    v: u32,
    correlation_id: String,
    records: Vec<RecordWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadWire {
    v: u32,
    correlation_id: String,
}

#[derive(Debug)]
pub(crate) struct WriteRequest {
    pub(crate) correlation: String,
    pub(crate) records: Vec<IntentRecord>,
}

#[derive(Debug)]
pub(crate) struct ReadRequest {
    pub(crate) correlation: String,
}

/// Owned parse failure: validated correlation (or `-`) plus a fixed reason.
#[derive(Debug)]
pub(crate) struct RequestParseError {
    pub(crate) correlation: String,
    pub(crate) reason: &'static str,
}

impl RequestParseError {
    fn new(correlation: &str, reason: &'static str) -> Self {
        Self {
            correlation: correlation.to_owned(),
            reason,
        }
    }
}

fn valid_wire(v: u32, correlation_id: &str) -> Result<String, RequestParseError> {
    if v != 2 || !valid_correlation(correlation_id) {
        return Err(RequestParseError::new("-", "invalid-request"));
    }
    Ok(correlation_id.to_owned())
}

/// Structural failures are `invalid-request`; over-count, duplicate subjects,
/// un-normalizable subjects, and overbound data are `invalid-members`
/// (subject/data content is validated by the store write, also
/// `invalid-members`).
pub(crate) fn parse_write_request(body: &str) -> Result<WriteRequest, RequestParseError> {
    let wire: WriteWire =
        serde_json::from_str(body).map_err(|_| RequestParseError::new("-", "invalid-request"))?;
    let correlation = valid_wire(wire.v, &wire.correlation_id)?;
    if wire.records.len() > MAX_INTENT_RECORDS {
        return Err(RequestParseError::new(&correlation, "invalid-members"));
    }
    let mut records = Vec::with_capacity(wire.records.len());
    for entry in &wire.records {
        let (Some(win), Some(data)) = (entry.win.as_str(), entry.data.as_str()) else {
            return Err(RequestParseError::new(&correlation, "invalid-request"));
        };
        records.push(IntentRecord {
            win: win.to_owned(),
            data: data.to_owned(),
        });
    }
    // Fail before disk on duplicate subjects, bad subjects, and overbound
    // data; the store re-checks the same predicate before replacing.
    validate_records(&records).map_err(|reason| RequestParseError::new(&correlation, reason))?;
    Ok(WriteRequest {
        correlation,
        records,
    })
}

/// Closed read shape: `{"v":2,"correlation_id":..}` only. Any extra key
/// (including the retired `live` inventory) rejects as `invalid-request`.
pub(crate) fn parse_read_request(body: &str) -> Result<ReadRequest, RequestParseError> {
    let wire: ReadWire =
        serde_json::from_str(body).map_err(|_| RequestParseError::new("-", "invalid-request"))?;
    let correlation = valid_wire(wire.v, &wire.correlation_id)?;
    Ok(ReadRequest { correlation })
}

/// Fixed-shape in-band rejection. Inputs must be validated correlation (or
/// `-`) and a fixed reason.
pub(crate) fn intent_rejection(correlation: &str, reason: &str) -> String {
    serde_json::json!({
        "v": 2,
        "correlation_id": sanitize_correlation(correlation),
        "outcome": "rejected",
        "records": [],
        "reason": reason,
    })
    .to_string()
}

pub(crate) fn write_stored_reply(correlation: &str, stored: usize) -> String {
    serde_json::json!({
        "v": 2,
        "correlation_id": sanitize_correlation(correlation),
        "outcome": "stored",
        "stored": stored,
    })
    .to_string()
}

/// `outcome` is `rejected` or `unavailable`; the reason must be a fixed label.
pub(crate) fn write_failed_reply(correlation: &str, outcome: &str, reason: &str) -> String {
    serde_json::json!({
        "v": 2,
        "correlation_id": sanitize_correlation(correlation),
        "outcome": outcome,
        "reason": reason,
    })
    .to_string()
}

pub(crate) fn read_ok_reply(correlation: &str, records: &[IntentRecord], stored: usize) -> String {
    serde_json::json!({
        "v": 2,
        "correlation_id": sanitize_correlation(correlation),
        "outcome": "ok",
        "records": records,
        "stored": stored,
        "returned": records.len(),
    })
    .to_string()
}

/// Degraded reply: empty records with a fixed reason and zero counts.
pub(crate) fn read_degraded_reply(correlation: &str, reason: &str) -> String {
    serde_json::json!({
        "v": 2,
        "correlation_id": sanitize_correlation(correlation),
        "outcome": "degraded",
        "records": [],
        "stored": 0,
        "returned": 0,
        "reason": reason,
    })
    .to_string()
}

/// Bounded terminal summary over counts and fixed labels only: no ids,
/// owners, bus ids, paths, data, payloads, or error strings.
pub(crate) fn intent_egress_summary(
    op: &str,
    correlation: &str,
    outcome: &str,
    stored: Option<usize>,
    returned: Option<usize>,
    reason: Option<&str>,
) -> String {
    format!(
        "omnitiler:intent-summary direction=egress op={op} correlation={} outcome={outcome} stored={} returned={} reason={}",
        sanitize_correlation(correlation),
        stored.map_or_else(|| "-".to_owned(), |count| count.to_string()),
        returned.map_or_else(|| "-".to_owned(), |count| count.to_string()),
        reason.unwrap_or("-"),
    )
}

/// Best-effort diagnostic writer: the write result is discarded so logging
/// never changes intent behavior.
pub(crate) fn emit_intent_diag(line: &str) {
    let _ = writeln!(std::io::stderr(), "{line}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::AtomicU64;

    static TEST_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_root(name: &str) -> PathBuf {
        let id = TEST_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("pat-intent-{name}-{}-{id}", std::process::id()));
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

    fn record(win: &str, data: &str) -> IntentRecord {
        IntentRecord {
            win: win.to_owned(),
            data: data.to_owned(),
        }
    }

    fn canonical_data() -> String {
        "{\"v\":1,\"float\":true,\"stickyPrior\":null}".to_owned()
    }

    fn project_fd_for(root: &Path) -> OwnedFd {
        let root_fd = open_root(root).expect("root opens");
        match open_project(&root_fd).expect("project opens or missing") {
            Some(fd) => fd,
            None => create_project(&root_fd).expect("project created"),
        }
    }

    /// Raw file plant for corruption fixtures: test setup only, never the
    /// production path (which only writes through `atomic_replace`).
    fn plant_file(root: &Path, body: &[u8]) {
        let project = project_fd_for(root);
        let fd = rustix::fs::openat(
            &project,
            STORE_FILE_NAME,
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
    fn correlation_gate_matches_core_alphabet() {
        assert!(valid_correlation("corr-1a_Z.9"));
        assert!(!valid_correlation(""));
        assert!(!valid_correlation("bad corr"));
        assert!(!valid_correlation("evil\nline"));
        assert!(!valid_correlation(&"x".repeat(MAX_CORRELATION_LEN + 1)));
        assert_eq!(sanitize_correlation("ok-1"), "ok-1");
        assert_eq!(sanitize_correlation("evil id"), "-");
    }

    #[test]
    fn member_normalization_matches_kwin_utility() {
        assert_eq!(
            normalize_member_id("abc-XYZ_0.9"),
            Some("abc-XYZ_0.9".to_owned())
        );
        assert_eq!(
            normalize_member_id("{12345678-1234-1234-1234-1234567890ab}"),
            Some("12345678-1234-1234-1234-1234567890ab".to_owned())
        );
        assert_eq!(normalize_member_id(""), None);
        assert_eq!(normalize_member_id("has space"), None);
        assert_eq!(normalize_member_id("has/slash"), None);
        assert_eq!(normalize_member_id("{not-a-uuid}"), None);
        assert_eq!(
            normalize_member_id(&"x".repeat(MAX_MEMBER_ID_LEN + 1)),
            None
        );
    }

    #[test]
    fn namespace_parse_is_bounded_and_authorizes_sender() {
        assert!(IntentNamespace::parse("bus-id-1", ":1.7").is_some());
        assert!(IntentNamespace::parse("", ":1.7").is_none());
        assert!(IntentNamespace::parse("bus id", ":1.7").is_none());
        assert!(IntentNamespace::parse("bus/id", ":1.7").is_none());
        assert!(IntentNamespace::parse("bus-id-1", "not-a-unique-name").is_none());
        let ns = namespace("bus-id-1", ":1.7");
        assert!(ns.authorizes_sender(":1.7"));
        assert!(!ns.authorizes_sender(":1.8"));
        assert!(!ns.authorizes_sender("not-unique"));
    }

    #[test]
    fn roundtrip_survives_service_restart_exact_bytes() {
        let root = fresh_root("restart");
        let ns = namespace("bus-restart", ":1.7");
        let data = canonical_data();
        let records = vec![record("win-a", &data), record("win-b", &data)];
        assert_eq!(
            FloatIntentStore::with_root(root.clone()).write_records(&ns, &records),
            Ok(2)
        );
        // Within-session restart persists the snapshot byte-for-byte before
        // the first admissible report: same records in the same order.
        let read = FloatIntentStore::with_root(root.clone()).read_membership(&ns);
        assert_eq!(read.degraded, None);
        assert_eq!(read.stored, 2);
        assert_eq!(read.records, records);
        drop_root(&root);
    }

    #[test]
    fn opaque_control_escapes_roundtrip() {
        let root = fresh_root("escapes");
        let ns = namespace("bus-escapes", ":1.7");
        // Exact opaque bytes with control escapes survive unchanged: the
        // store never interprets inner JSON.
        let data = "{\"v\":1,\"float\":null,\"stickyPrior\":null}\u{0009}".to_owned();
        let records = vec![record("win-a", &data)];
        FloatIntentStore::with_root(root.clone())
            .write_records(&ns, &records)
            .expect("write");
        let read = FloatIntentStore::with_root(root.clone()).read_membership(&ns);
        assert_eq!(read.degraded, None);
        assert_eq!(read.records, records);
        drop_root(&root);
    }

    #[test]
    fn missing_store_reads_plain_empty() {
        let root = fresh_root("missing");
        let read =
            FloatIntentStore::with_root(root.clone()).read_membership(&namespace("bus-m", ":1.7"));
        assert_eq!(read.degraded, None);
        assert_eq!(read.stored, 0);
        assert!(read.records.is_empty());
        drop_root(&root);
    }

    #[test]
    fn namespace_mismatch_degrades_empty_and_write_replaces() {
        let root = fresh_root("ns");
        let store = FloatIntentStore::with_root(root.clone());
        let first = namespace("bus-one", ":1.7");
        let new_bus = namespace("bus-two", ":1.7");
        let new_owner = namespace("bus-one", ":1.9");
        store
            .write_records(&first, &[record("win-a", &canonical_data())])
            .expect("write");
        for other in [&new_bus, &new_owner] {
            let read = store.read_membership(other);
            assert_eq!(read.degraded, Some("namespace-mismatch"));
            assert!(read.records.is_empty());
        }
        assert_eq!(
            store.write_records(&new_bus, &[record("win-b", &canonical_data())]),
            Ok(1)
        );
        assert_eq!(
            store.read_membership(&new_bus).records,
            vec![record("win-b", &canonical_data())]
        );
        drop_root(&root);
    }

    #[test]
    fn corrupt_and_oversize_and_bad_entries_degrade() {
        let root = fresh_root("corrupt");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-corrupt", ":1.7");
        let over_data = "x".repeat(MAX_INTENT_DATA_BYTES + 1);
        let bad = [
            "not json{{{".to_owned(),
            r#"{"v":2,"bus":"bus-corrupt","kwin":":1.7","records":{}}"#.to_owned(),
            r#"{"v":2,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":42,"data":"x"}]}"#
                .to_owned(),
            r#"{"v":2,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"","data":"x"}]}"#
                .to_owned(),
            r#"{"v":2,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"has space","data":"x"}]}"#
                .to_owned(),
            r#"{"v":2,"bus":"bus-corrupt","kwin":":1.7","records":[{"win":"a","data":"x"},{"win":"a","data":"y"}]}"#
                .to_owned(),
            format!(
                r#"{{"v":2,"bus":"bus-corrupt","kwin":":1.7","records":[{{"win":"a","data":"{over_data}"}}]}}"#
            ),
        ];
        for body in &bad {
            plant_file(&root, body.as_bytes());
            assert_eq!(
                store.read_membership(&ns).degraded,
                Some("corrupt"),
                "{body}"
            );
        }
        plant_file(&root, &vec![b'x'; MAX_STORE_FILE_BYTES as usize + 1]);
        assert_eq!(store.read_membership(&ns).degraded, Some("unreadable"));
        assert_eq!(
            store.write_records(&ns, &[record("win-a", &canonical_data())]),
            Ok(1)
        );
        assert_eq!(
            store.read_membership(&ns).records,
            vec![record("win-a", &canonical_data())]
        );
        drop_root(&root);
    }

    #[test]
    fn old_v1_format_degrades_corrupt_and_next_write_replaces() {
        let root = fresh_root("v1-reject");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-v1", ":1.7");
        // The rejected v1 membership schema degrades through the existing
        // empty path with a fixed reason; no conversion is attempted.
        plant_file(
            &root,
            br#"{"v":1,"bus":"bus-v1","kwin":":1.7","members":["win-a"],"tile":["win-t"]}"#,
        );
        let read = store.read_membership(&ns);
        assert_eq!(read.degraded, Some("corrupt"));
        assert!(read.records.is_empty());
        assert_eq!(read.stored, 0);
        // The next full v2 snapshot replaces the old file atomically.
        let records = vec![record("win-b", &canonical_data())];
        assert_eq!(store.write_records(&ns, &records), Ok(1));
        let kept = store.read_membership(&ns);
        assert_eq!(kept.degraded, None);
        assert_eq!(kept.records, records);
        drop_root(&root);
    }

    #[test]
    fn explicit_replacement_and_clear_snapshot() {
        let root = fresh_root("replace");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-replace", ":1.7");
        let data = canonical_data();
        store
            .write_records(&ns, &[record("win-a", &data)])
            .expect("first");
        // Explicit replacement swaps the full snapshot: win-a is gone.
        let second = vec![record("win-b", &data), record("win-c", &data)];
        assert_eq!(store.write_records(&ns, &second), Ok(2));
        assert_eq!(store.read_membership(&ns).records, second);
        // Empty snapshot clears.
        assert_eq!(store.write_records(&ns, &[]), Ok(0));
        let read = store.read_membership(&ns);
        assert_eq!(read.degraded, None);
        assert_eq!(read.stored, 0);
        assert!(read.records.is_empty());
        drop_root(&root);
    }

    #[test]
    fn write_validates_before_touching_disk() {
        let root = fresh_root("validate");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-validate", ":1.7");
        let data = canonical_data();
        let over_data = "x".repeat(MAX_INTENT_DATA_BYTES + 1);
        assert_eq!(
            store.write_records(&ns, &[record("bad id", &data)]),
            Err("invalid-members")
        );
        assert_eq!(
            store.write_records(&ns, &[record("a", &data), record("a", &data)]),
            Err("invalid-members")
        );
        assert_eq!(
            store.write_records(&ns, &[record("a", &over_data)]),
            Err("invalid-members")
        );
        assert_eq!(
            store.write_records(&ns, &vec![record("a", &data); MAX_INTENT_RECORDS + 1]),
            Err("invalid-members")
        );
        let read = store.read_membership(&ns);
        assert_eq!(read.degraded, None);
        assert!(read.records.is_empty());
        drop_root(&root);
    }

    #[test]
    fn created_modes_are_private_and_wrong_modes_refuse() {
        let root = fresh_root("perms");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-perms", ":1.7");
        store
            .write_records(&ns, &[record("win-a", &canonical_data())])
            .expect("write");
        let mode_of = |fd: &OwnedFd| rustix::fs::fstat(fd).expect("fstat").st_mode & 0o777;
        let root_fd = open_root(&root).expect("root opens");
        assert_eq!(mode_of(&root_fd), 0o700);
        // Nothing is ever chmodded: loosening the root refuses and the mode
        // bits are untouched.
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).expect("loosen");
        assert_eq!(store.read_membership(&ns).degraded, Some("unreadable"));
        assert_eq!(
            store.write_records(&ns, &[record("win-a", &canonical_data())]),
            Err("store-unavailable")
        );
        assert_eq!(
            std::fs::symlink_metadata(&root)
                .expect("meta")
                .permissions()
                .mode()
                & 0o777,
            0o755,
            "root must never be chmodded"
        );
        drop_root(&root);
    }

    #[test]
    fn wrong_mode_project_dir_refuses_without_chmod() {
        let root = fresh_root("dirperm");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-dirperm", ":1.7");
        store
            .write_records(&ns, &[record("win-a", &canonical_data())])
            .expect("write");
        let project = root.join(STORE_DIR_NAME);
        std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o755)).expect("loosen");
        assert_eq!(store.read_membership(&ns).degraded, Some("unreadable"));
        assert_eq!(
            store.write_records(&ns, &[record("win-b", &canonical_data())]),
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
    fn symlinks_are_refused_and_foreign_data_survives() {
        let root = fresh_root("symlink");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-symlink", ":1.7");
        let foreign = root.join("foreign.json");
        let sentinel = br#"{"v":2,"bus":"bus-symlink","kwin":":1.7","records":[]}"#;
        std::fs::write(&foreign, sentinel).expect("foreign");
        // Ensure the project dir exists for the path-based link plant below;
        // production access itself stays anchored.
        let _ = project_fd_for(&root);
        std::os::unix::fs::symlink(
            &foreign,
            root.join(format!("{STORE_DIR_NAME}/{STORE_FILE_NAME}")),
        )
        .expect("file link");
        // The anchored open refuses the linked final component: the foreign
        // bytes are never read and a write never follows the link.
        let read = store.read_membership(&ns);
        assert_eq!(read.degraded, Some("unreadable"));
        assert!(read.records.is_empty());
        assert_eq!(
            store.write_records(&ns, &[record("win-a", &canonical_data())]),
            Err("store-unavailable")
        );
        assert_eq!(std::fs::read(&foreign).expect("foreign"), sentinel);
        // A linked project dir refuses as well.
        let root2 = fresh_root("symlink-dir");
        let elsewhere = root2.join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, root2.join(STORE_DIR_NAME)).expect("dir link");
        assert_eq!(
            FloatIntentStore::with_root(root2.clone())
                .read_membership(&namespace("bus-symlink", ":1.7"))
                .degraded,
            Some("unreadable")
        );
        // A linked root refuses as well.
        let root3 = fresh_root("symlink-root");
        let link = root3.join("link-root");
        std::os::unix::fs::symlink(&root, &link).expect("root link");
        assert_eq!(
            FloatIntentStore::with_root(link)
                .read_membership(&ns)
                .degraded,
            Some("unreadable")
        );
        drop_root(&root);
        drop_root(&root2);
        drop_root(&root3);
    }

    #[test]
    fn nested_ancestor_symlink_refuses_without_foreign_changes() {
        let outer = fresh_root("ancestors");
        // Real chain with a sentinel proving it is never written through.
        let real_sub = outer.join("real").join("sub");
        std::fs::create_dir_all(&real_sub).expect("real chain");
        std::fs::set_permissions(&real_sub, std::fs::Permissions::from_mode(0o700))
            .expect("strict sub");
        let sentinel = outer.join("real").join("sentinel.txt");
        std::fs::write(&sentinel, b"sentinel-bytes").expect("sentinel");
        // Nested link in the middle of the addressed chain.
        let wrap = outer.join("wrap");
        std::fs::create_dir(&wrap).expect("wrap");
        std::os::unix::fs::symlink(outer.join("real"), wrap.join("link")).expect("nested link");
        let via_link = wrap.join("link").join("sub");
        let store = FloatIntentStore::with_root(via_link);
        let ns = namespace("bus-ancestors", ":1.7");
        // The component walk refuses the linked ancestor with ELOOP before
        // any gate or read, so nothing under the real chain is created or
        // modified.
        let read = store.read_membership(&ns);
        assert_eq!(read.degraded, Some("unreadable"));
        assert!(read.records.is_empty());
        assert_eq!(
            store.write_records(&ns, &[record("win-a", &canonical_data())]),
            Err("store-unavailable")
        );
        assert!(
            wrap.join("link")
                .symlink_metadata()
                .expect("meta")
                .file_type()
                .is_symlink(),
            "nested link itself is untouched"
        );
        assert_eq!(
            std::fs::read(&sentinel).expect("sentinel"),
            b"sentinel-bytes"
        );
        assert!(
            !real_sub.join(STORE_DIR_NAME).exists(),
            "no project dir is created through the refused chain"
        );
        drop_root(&outer);
    }

    #[test]
    fn relative_and_dotdot_roots_refuse_without_touching_disk() {
        let ns = namespace("bus-relroot", ":1.7");
        let data = canonical_data();
        let relative = FloatIntentStore::with_root(PathBuf::from("relative-intent-root"));
        assert_eq!(relative.read_membership(&ns).degraded, Some("unreadable"));
        assert_eq!(
            relative.write_records(&ns, &[record("win-a", &data)]),
            Err("store-unavailable")
        );
        assert!(!Path::new("relative-intent-root").exists());
        // An absolute path that only resolves through `..` still refuses:
        // the walk rejects the ParentDir component before resolution.
        let sibling = fresh_root("dotdot-sib");
        let target = fresh_root("dotdot-target");
        let via_dotdot = sibling.join("..").join(target.file_name().expect("name"));
        let store = FloatIntentStore::with_root(via_dotdot);
        assert_eq!(store.read_membership(&ns).degraded, Some("unreadable"));
        assert_eq!(
            store.write_records(&ns, &[record("win-a", &data)]),
            Err("store-unavailable")
        );
        assert!(!target.join(STORE_DIR_NAME).exists());
        drop_root(&sibling);
        drop_root(&target);
    }

    #[test]
    fn planted_fifo_degrades_and_fails_without_blocking() {
        let root = fresh_root("fifo");
        let store = FloatIntentStore::with_root(root.clone());
        let ns = namespace("bus-fifo", ":1.7");
        let project = project_fd_for(&root);
        rustix::fs::mkfifoat(&project, STORE_FILE_NAME, Mode::from_bits_truncate(0o600))
            .expect("fifo plant");
        drop(project);
        // O_NONBLOCK on the final-file opens means the gate open returns
        // immediately instead of blocking on the FIFO; fstat then rejects
        // the non-regular type, so the read degrades and the write fails.
        let read = store.read_membership(&ns);
        assert_eq!(read.degraded, Some("unreadable"));
        assert!(read.records.is_empty());
        assert_eq!(
            store.write_records(&ns, &[record("win-a", &canonical_data())]),
            Err("store-unavailable")
        );
        drop_root(&root);
    }

    #[test]
    fn occupied_temp_is_preserved_byte_for_byte() {
        let root = fresh_root("tempcollide");
        let project = project_fd_for(&root);
        // A pre-existing file at the temp name fails exclusive creation and
        // is preserved byte-for-byte; nothing is unlinked.
        let occupant = b"occupant-bytes";
        let temp = "fixed-temp-name";
        let fd = rustix::fs::openat(
            &project,
            temp,
            OFlags::WRONLY
                .union(OFlags::CREATE)
                .union(OFlags::EXCL)
                .union(OFlags::CLOEXEC),
            Mode::from_bits_truncate(0o600),
        )
        .expect("plant occupant");
        let mut file = std::fs::File::from(fd);
        file.write_all(occupant).expect("plant bytes");
        drop(file);
        assert_eq!(
            atomic_replace(&project, b"{}", temp, STORE_FILE_NAME),
            Err(())
        );
        let fd = rustix::fs::openat(&project, temp, FILE_FLAGS, Mode::empty()).expect("reopen");
        let mut kept = Vec::new();
        std::fs::File::from(fd)
            .read_to_end(&mut kept)
            .expect("reread");
        assert_eq!(kept, occupant);
        // A pre-existing symlink at the temp name is likewise preserved (no
        // unlink, no follow).
        let link_temp = "fixed-temp-link";
        let target = root.join("link-target.json");
        std::fs::write(&target, b"target-bytes").expect("target");
        std::os::unix::fs::symlink(&target, root.join(format!("{STORE_DIR_NAME}/{link_temp}")))
            .expect("temp link");
        assert_eq!(
            atomic_replace(&project, b"{}", link_temp, STORE_FILE_NAME),
            Err(())
        );
        assert!(
            std::fs::symlink_metadata(root.join(format!("{STORE_DIR_NAME}/{link_temp}")))
                .expect("meta")
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(&target).expect("target"), b"target-bytes");
        drop(project);
        drop_root(&root);
    }

    #[test]
    fn successful_writes_leave_no_temp_files() {
        let root = fresh_root("notemp");
        let store = FloatIntentStore::with_root(root.clone());
        store
            .write_records(
                &namespace("bus-notemp", ":1.7"),
                &[record("win-a", &canonical_data())],
            )
            .expect("write");
        let leftovers: Vec<_> = std::fs::read_dir(root.join(STORE_DIR_NAME))
            .expect("list dir")
            .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
            .filter(|name| name.to_string_lossy().starts_with(TEMP_FILE_PREFIX))
            .collect();
        assert!(leftovers.is_empty(), "no temp files remain: {leftovers:?}");
        drop_root(&root);
    }

    #[test]
    fn summaries_carry_counts_never_ids() {
        let line = intent_egress_summary("read", "evil-id\n:1.999", "ok", Some(2), Some(1), None);
        assert!(line.contains("correlation=-"), "{line}");
        assert!(line.contains("stored=2"), "{line}");
        assert!(line.contains("returned=1"), "{line}");
        assert!(!line.contains("evil"), "{line}");
        assert!(!line.contains(":1.999"), "{line}");
        let line = intent_egress_summary(
            "write",
            "corr-1",
            "degraded",
            Some(0),
            Some(0),
            Some("corrupt"),
        );
        assert!(line.contains("correlation=corr-1"), "{line}");
        assert!(line.contains("reason=corrupt"), "{line}");
        assert!(!line.contains('\n'), "{line}");
    }

    #[test]
    fn wire_replies_are_bounded_fixed_shapes() {
        let ok = write_stored_reply("corr-1", 2);
        let parsed: serde_json::Value = serde_json::from_str(&ok).expect("valid JSON");
        assert_eq!(parsed["v"], 2);
        assert_eq!(parsed["outcome"], "stored");
        assert_eq!(parsed["stored"], 2);
        assert!(ok.len() <= INTENT_MAX_REPLY_BYTES);
        let rejected = intent_rejection("evil corr", "unauthorized");
        assert!(rejected.contains("\"correlation_id\":\"-\""), "{rejected}");
        assert!(!rejected.contains("evil"), "{rejected}");
        let degraded = read_degraded_reply("corr-1", "corrupt");
        let parsed: serde_json::Value = serde_json::from_str(&degraded).expect("valid JSON");
        assert_eq!(parsed["v"], 2);
        assert_eq!(parsed["outcome"], "degraded");
        assert_eq!(parsed["records"].as_array().expect("array").len(), 0);
        assert_eq!(parsed["stored"], 0);
        assert_eq!(parsed["returned"], 0);
        let ok = read_ok_reply("corr-1", &[record("win-a", &canonical_data())], 1);
        let parsed: serde_json::Value = serde_json::from_str(&ok).expect("valid JSON");
        assert_eq!(parsed["v"], 2);
        assert_eq!(parsed["stored"], 1);
        assert_eq!(parsed["returned"], 1);
        assert_eq!(parsed["records"].as_array().expect("array").len(), 1);
    }

    #[test]
    fn write_request_parsing_separates_structural_from_semantic() {
        let good = parse_write_request(
            r#"{"v":2,"correlation_id":"corr-1","records":[{"win":"win-a","data":"x"}]}"#,
        )
        .expect("valid");
        assert_eq!(good.correlation, "corr-1");
        assert_eq!(good.records, vec![record("win-a", "x")]);
        for body in [
            "not json",
            r#"{"v":2,"records":[]}"#,
            r#"{"v":2,"correlation_id":"bad corr","records":[]}"#,
            r#"{"v":1,"correlation_id":"corr-1","records":[]}"#,
            r#"{"v":2,"correlation_id":"corr-1"}"#,
            r#"{"v":2,"correlation_id":"corr-1","records":{}}"#,
            r#"{"v":2,"correlation_id":"corr-1","records":[{"win":"win-a"}]}"#,
            r#"{"v":2,"correlation_id":"corr-1","records":[{"win":"win-a","data":42}]}"#,
        ] {
            let error = parse_write_request(body).expect_err("must reject");
            assert_eq!(error.reason, "invalid-request", "{body}");
            assert!(!error.correlation.contains("bad"), "{body}");
        }
        let clear = parse_write_request(r#"{"v":2,"correlation_id":"corr-1","records":[]}"#)
            .expect("empty clears");
        assert!(clear.records.is_empty());
        // Semantic failures surface as invalid-members before disk.
        for body in [
            r#"{"v":2,"correlation_id":"corr-1","records":[{"win":"bad id","data":"x"}]}"#,
            r#"{"v":2,"correlation_id":"corr-1","records":[{"win":"a","data":"x"},{"win":"a","data":"y"}]}"#,
        ] {
            let error = parse_write_request(body).expect_err("must reject");
            assert_eq!(error.reason, "invalid-members", "{body}");
        }
    }

    #[test]
    fn read_request_parsing_is_closed_v2_only() {
        let plain = parse_read_request(r#"{"v":2,"correlation_id":"corr-1"}"#).expect("valid");
        assert_eq!(plain.correlation, "corr-1");
        // The retired live inventory and any extra key reject.
        for body in [
            r#"{"v":2,"correlation_id":"corr-1","live":["win-a"]}"#,
            r#"{"v":2,"correlation_id":"corr-1","records":[]}"#,
            r#"{"v":1,"correlation_id":"corr-1"}"#,
            r#"{"v":2,"correlation_id":"bad corr"}"#,
            r#"{"v":2}"#,
        ] {
            assert!(parse_read_request(body).is_err(), "{body}");
        }
    }
}
