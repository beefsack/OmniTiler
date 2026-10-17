#![forbid(unsafe_code)]
use crate::model::{LedgerError, RecoveryLedger, parse_ledger, validate_ledger};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const LEDGER_FILE_NAME: &str = "ledger.json";
pub const LOCK_FILE_NAME: &str = "ledger.lock";
pub const PENDING_FILE_NAME: &str = "ledger.json.pending";

#[derive(Debug)]
pub enum StorageError {
    Io(std::io::Error),
    Ledger(LedgerError),
    DifferentOwner,
    LockContended,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(_) => f.write_str("io-error"),
            Self::Ledger(inner) => write!(f, "{inner}"),
            Self::DifferentOwner => f.write_str("different-owner"),
            Self::LockContended => f.write_str("lock-contended"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(inner) => Some(inner),
            Self::Ledger(inner) => Some(inner),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StorageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(windows)]
pub(crate) fn atomic_replace(from: &Path, to: &Path) -> Result<(), StorageError> {
    crate::ffi::move_file_replace(from, to).map_err(StorageError::Io)
}

#[cfg(not(windows))]
pub(crate) fn atomic_replace(from: &Path, to: &Path) -> Result<(), StorageError> {
    std::fs::rename(from, to).map_err(StorageError::Io)
}

/// Bounded single-pending publish failure: either the final name is already
/// taken (another request pending) or a real I/O error occurred.
#[derive(Debug)]
pub enum PublishError {
    /// The final name already exists: the pending queue is full. The caller's
    /// temp body was dropped; the existing final bytes are untouched.
    Pending,
    Io(std::io::Error),
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Io(_) => f.write_str("io-error"),
        }
    }
}

impl std::error::Error for PublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pending => None,
            Self::Io(inner) => Some(inner),
        }
    }
}

fn publish_tmp_name(final_name: &str, tmp_tag: &str, attempt: u64) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let tag: String = tmp_tag
        .bytes()
        .filter(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_' || *b == b'.')
        .take(32)
        .map(char::from)
        .collect();
    format!(
        "{final_name}.{}-{nanos}-{count}-{attempt}.tmp",
        if tag.is_empty() {
            std::process::id().to_string()
        } else {
            format!("{}-{tag}", std::process::id())
        }
    )
}

/// Publish `bytes` to `dir/final_name` without ever exposing a partial body
/// and without overwriting an existing final: the body is written and synced
/// to a same-directory unique temp (created with `create_new`) and only then
/// published atomically with no-replace semantics (Windows `MoveFileExW`
/// without `REPLACE_EXISTING`; elsewhere a `hard_link`). A lost publish race
/// reports [`PublishError::Pending`] with the existing final untouched. The
/// caller's own temp is removed on every error path; only the winner's bytes
/// ever appear under the final name, complete and synced.
pub fn publish_no_overwrite(
    dir: &Path,
    final_name: &str,
    bytes: &[u8],
    tmp_tag: &str,
) -> Result<(), PublishError> {
    let tmp = write_tmp(dir, final_name, bytes, tmp_tag)?;
    #[cfg(windows)]
    {
        // No REPLACE_EXISTING: a present final fails here instead of being
        // overwritten, so the queued body is never torn or replaced.
        match crate::ffi::move_file_no_replace(&tmp, &dir.join(final_name)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                if e.kind() == std::io::ErrorKind::AlreadyExists || e.raw_os_error() == Some(183) {
                    Err(PublishError::Pending)
                } else {
                    Err(PublishError::Io(e))
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        let final_path = dir.join(final_name);
        match std::fs::hard_link(&tmp, &final_path) {
            Ok(()) => {
                let _ = std::fs::remove_file(&tmp);
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Err(PublishError::Pending)
                } else {
                    Err(PublishError::Io(e))
                }
            }
        }
    }
}

/// Publish `bytes` to `dir/final_name` without ever exposing a partial body,
/// replacing any existing final atomically: same synced same-directory temp
/// as [`publish_no_overwrite`], committed with replace semantics (Windows
/// `MoveFileExW` with `REPLACE_EXISTING | WRITE_THROUGH`; elsewhere a
/// rename). Readers either see the old complete body or the new complete
/// body, never a mix. For request files whose publishers must stay visible
/// to a file-name-only directory watch (e.g. the stop request): the
/// signaled rename always exposes complete content, with overwrite
/// preserved for repeated requests.
pub fn publish_replace(
    dir: &Path,
    final_name: &str,
    bytes: &[u8],
    tmp_tag: &str,
) -> Result<(), PublishError> {
    let tmp = write_tmp(dir, final_name, bytes, tmp_tag)?;
    #[cfg(windows)]
    {
        match crate::ffi::move_file_replace(&tmp, &dir.join(final_name)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(PublishError::Io(e))
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Err(e) = std::fs::rename(&tmp, dir.join(final_name)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(PublishError::Io(e));
        }
        Ok(())
    }
}

/// Validated synced same-directory temp body shared by both publishers.
/// Returns the temp path; removes it on every error path.
fn write_tmp(
    dir: &Path,
    final_name: &str,
    bytes: &[u8],
    tmp_tag: &str,
) -> Result<PathBuf, PublishError> {
    if final_name.is_empty()
        || final_name.contains(['/', '\\'])
        || final_name == "."
        || final_name == ".."
    {
        return Err(PublishError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid final name",
        )));
    }
    std::fs::create_dir_all(dir).map_err(PublishError::Io)?;
    let mut attempt = 0u64;
    let tmp = loop {
        let name = publish_tmp_name(final_name, tmp_tag, attempt);
        let path = dir.join(&name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(e) = (|| -> std::io::Result<()> {
                    use std::io::Write;
                    file.write_all(bytes)?;
                    file.sync_all()?;
                    Ok(())
                })() {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(PublishError::Io(e));
                }
                drop(file);
                break path;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 8 => {
                attempt += 1;
            }
            Err(e) => return Err(PublishError::Io(e)),
        }
    };
    Ok(tmp)
}

pub struct LedgerStore {
    dir: PathBuf,
    _lock: File,
}

impl LedgerStore {
    pub fn open(dir: &Path) -> Result<Self, StorageError> {
        std::fs::create_dir_all(dir)?;
        let lock = File::create(dir.join(LOCK_FILE_NAME))?;
        lock.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => StorageError::LockContended,
            std::fs::TryLockError::Error(io) => StorageError::Io(io),
        })?;
        Ok(Self {
            dir: dir.to_path_buf(),
            _lock: lock,
        })
    }

    pub fn committed(&self) -> Result<Option<RecoveryLedger>, StorageError> {
        let path = self.dir.join(LEDGER_FILE_NAME);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let text = std::str::from_utf8(&bytes)
                    .map_err(|_| StorageError::Ledger(LedgerError::Malformed))?;
                Ok(Some(parse_ledger(text).map_err(StorageError::Ledger)?))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    pub fn commit(&self, ledger: &RecoveryLedger) -> Result<(), StorageError> {
        validate_ledger(ledger).map_err(StorageError::Ledger)?;
        let committed = self.dir.join(LEDGER_FILE_NAME);
        if self
            .committed()?
            .is_some_and(|existing| existing.owner != ledger.owner)
        {
            return Err(StorageError::DifferentOwner);
        }
        let bytes = serde_json::to_vec_pretty(ledger)
            .map_err(|_| StorageError::Ledger(LedgerError::Malformed))?;
        let pending = self.dir.join(PENDING_FILE_NAME);
        let mut file = File::create(&pending)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        atomic_replace(&pending, &committed)?;
        Ok(())
    }
}
