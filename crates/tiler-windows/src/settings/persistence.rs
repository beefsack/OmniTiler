#![forbid(unsafe_code)]
use super::*;

/// Settings file path under a product directory.
#[must_use]
pub fn settings_file_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE_NAME)
}

/// Load and strictly validate the settings file. Missing files map to
/// defaults at revision 1 (the caller distinguishes via [`LoadOutcome`]).
fn load_validated(bytes: &[u8]) -> Result<Settings, SettingsError> {
    let text = std::str::from_utf8(bytes).map_err(|_| SettingsError::Malformed)?;
    let settings: Settings = serde_json::from_str(text).map_err(|_| SettingsError::Malformed)?;
    validate_settings(&settings)?;
    Ok(settings)
}

/// Load outcome for one directory read. Invalid files report their reason
/// and leave the bytes untouched (no blind reset, no rewrite).
#[derive(Debug, Clone, PartialEq)]
// Whole-document `Settings` travels by value through load/poll by design;
// boxing it would ripple through every owner call site for no behavior gain.
#[allow(clippy::large_enum_variant)]
pub enum LoadOutcome {
    Loaded(Settings),
    Missing,
    Invalid(SettingsError),
}

/// Load the settings file from `dir`. Bounded read (oversized files refuse
/// before allocating); strict validation.
pub fn load_from_dir(dir: &Path) -> LoadOutcome {
    let path = settings_file_path(dir);
    let metadata = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LoadOutcome::Missing,
        Err(e) => return LoadOutcome::Invalid(SettingsError::Io(e.to_string())),
    };
    if metadata.len() > SETTINGS_MAX_BYTES {
        return LoadOutcome::Invalid(SettingsError::TooLarge);
    }
    // Re-check after the bounded read: the file may have grown between the
    // metadata probe and the read, and `take` caps the allocation.
    let mut bytes = Vec::new();
    let read = (|| -> std::io::Result<()> {
        use std::io::Read;
        let file = std::fs::File::open(&path)?;
        file.take(SETTINGS_MAX_BYTES + 1).read_to_end(&mut bytes)?;
        Ok(())
    })();
    if let Err(e) = read {
        return LoadOutcome::Invalid(SettingsError::Io(e.to_string()));
    }
    if bytes.len() as u64 > SETTINGS_MAX_BYTES {
        return LoadOutcome::Invalid(SettingsError::TooLarge);
    }
    match load_validated(&bytes) {
        Ok(settings) => LoadOutcome::Loaded(settings),
        Err(e) => LoadOutcome::Invalid(e),
    }
}

/// Validate and atomically save settings to `dir`, bumping the revision past
/// both the file's and the caller's revision. The body goes to a unique
/// same-directory temp (created with `create_new`, so concurrent writers
/// never share a pending body) and is committed with the existing Win32
/// replace pattern on Windows (`MoveFileExW` with replace+write-through)
/// and same-directory rename elsewhere. The temp is removed on every error
/// path; only complete validated bytes ever appear under the final name.
pub fn save_to_dir(dir: &Path, settings: &mut Settings) -> Result<(), SettingsError> {
    validate_settings(settings)?;
    let existing_revision = match load_from_dir(dir) {
        LoadOutcome::Loaded(current) => current.revision,
        LoadOutcome::Missing => 0,
        LoadOutcome::Invalid(e) => return Err(e),
    };
    settings.revision = existing_revision.saturating_add(1).max(settings.revision);
    validate_settings(settings)?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| SettingsError::Malformed)?;
    std::fs::create_dir_all(dir).map_err(|e| SettingsError::Io(e.to_string()))?;
    static SAVE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut attempt = 0u32;
    let pending = loop {
        let count = SAVE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(1);
        let candidate = dir.join(format!(
            "{SETTINGS_FILE_NAME}.{}-{nanos}-{count}.pending",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                use std::io::Write;
                let write = (|| -> std::io::Result<()> {
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    Ok(())
                })();
                drop(file);
                if let Err(e) = write {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(SettingsError::Io(e.to_string()));
                }
                break candidate;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 8 => {
                attempt += 1;
            }
            Err(e) => return Err(SettingsError::Io(e.to_string())),
        }
    };
    if let Err(e) = crate::storage::atomic_replace(&pending, &settings_file_path(dir)) {
        let _ = std::fs::remove_file(&pending);
        return Err(SettingsError::Io(e.to_string()));
    }
    Ok(())
}

/// Last-good live state held by the owner across polls.
#[derive(Debug, Clone)]
pub struct LiveSettings {
    pub settings: Settings,
    pub mtime: Option<SystemTime>,
    /// Observable status for the later UI proof: `saved` carries the applied
    /// revision, `degraded` carries the bounded invalid-file reason while the
    /// last-good values stay live.
    pub status: String,
}

impl LiveSettings {
    #[must_use]
    pub fn fresh(settings: Settings, mtime: Option<SystemTime>) -> Self {
        let status = format!("saved:{}", settings.revision);
        Self {
            settings,
            mtime,
            status,
        }
    }
}

/// Poll outcome for one owner-pump check. `InvalidKept` keeps the last-good
/// values live with a degraded status; the file is never rewritten.
#[derive(Debug, Clone, PartialEq)]
// Same by-value document rationale as `LoadOutcome` above.
#[allow(clippy::large_enum_variant)]
pub enum PollOutcome {
    Unchanged,
    Changed(Settings),
    InvalidKept(String),
}

/// Poll `dir` for settings changes against the owner's last-good state.
/// Compares modification time first so unchanged files cost one stat; a
/// changed file reloads and validates, and an invalid file keeps last-good.
pub fn poll_for_change(dir: &Path, last: &LiveSettings) -> PollOutcome {
    let path = settings_file_path(dir);
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    if mtime == last.mtime && last.mtime.is_some() {
        return PollOutcome::Unchanged;
    }
    match load_from_dir(dir) {
        LoadOutcome::Loaded(settings) => {
            if (settings.revision == last.settings.revision && mtime == last.mtime)
                || settings == last.settings
            {
                PollOutcome::Unchanged
            } else {
                PollOutcome::Changed(settings)
            }
        }
        LoadOutcome::Missing => {
            if last.mtime.is_none() {
                PollOutcome::Unchanged
            } else {
                // The file vanished: keep last-good, stay degraded-visible.
                PollOutcome::InvalidKept("missing: keeping last saved settings".to_owned())
            }
        }
        LoadOutcome::Invalid(e) => PollOutcome::InvalidKept(e.to_string()),
    }
}
