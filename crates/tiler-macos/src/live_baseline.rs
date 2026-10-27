#![forbid(unsafe_code)]

//! Mandatory live-leg baselines for macOS fixture legs.
//!
//! Every fixture leg captures this baseline BEFORE any input, product startup,
//! or settings mutation, persists it atomically to the private per-user
//! recovery directory, and compares it on teardown. Any unreadable category
//! aborts the leg before anything is touched; any teardown mismatch aborts
//! with the residue reported and never silently passes, and user state is
//! never automatically overwritten.
//!
//! Categories (all required, exact values not counts):
//! - protected windows: raw PID, window ID, layer, onscreen, and bounds for
//!   every window owned by a protected bundle or Ghostty (title-free read);
//! - shortcut holders: exact per-holder session state (enabled plus the
//!   posting chord) for Desktops 1-9, the coverage Authentic startup ensures;
//! - accent: live resolved system-accent RGB (the `NSColor.controlAccentColor`
//!   resolution, always present on a readable session). An unreadable
//!   conversion refuses the whole category (never a fabricated absent
//!   preimage); schema scope is resolved RGB only, never raw preference
//!   keys, never a write.
//! - processes: the running-application table (PID plus bundle ID);
//! - native topology: separate-Spaces mode plus per-display Space order and
//!   current (every display needs a readable current: the goto-back target).
//!
//! Reads arrive through [`BaselineReads`] so tests script every refusal
//! without touching the desktop; production uses [`LiveBaselineReads`].
//! Persistence reuses the private-dir and atomic-publish hardening of the
//! Desktop-N preimage store (0700 dir, 0600 file, no-clobber link, parent
//! fsync, cross-process transaction lock).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Baseline file schema version.
pub const BASELINE_SCHEMA_VERSION: u32 = 1;

/// Recovery file name under the per-user application-support directory (the
/// same parent as the Desktop-N preimage, so the conventions cannot drift).
pub const BASELINE_FILE_NAME: &str = "live-leg-baseline.json";

/// Shortcut-holder coverage: Desktops 1-9, matching the Authentic startup
/// ensure ([`crate::workspace_send::ensure_authentic_holders`]).
pub const BASELINE_HOLDER_COUNT: u32 = 9;

/// One protected window: raw identity plus geometry. No titles anywhere on
/// this surface (the read itself is title-free). The capture is a
/// conservative superset of ALL pre-existing foreign windows: entries whose
/// owner bundle is unreadable carry `bundle: None` and are treated as
/// protected (never silently omitted). Teardown exclusions for leg-owned
/// windows apply only through the registered ownership list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtectedWindow {
    pub pid: u32,
    pub wid: u32,
    pub layer: i32,
    pub onscreen: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Owner bundle when readable; `None` means unreadable (conservatively
    /// protected, never dropped from the baseline).
    pub bundle: Option<String>,
    pub bundle_protected: bool,
    pub bundle_ghostty: bool,
}

/// One Desktop-N holder's session state: the posting chord plus its enabled
/// flag (the same subset the explicit Apply/Revert owns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolderBaseline {
    pub symbolic_id: u32,
    pub desktop: u32,
    pub enabled: bool,
    pub keycode: u32,
    pub modifiers: u64,
}

/// Live system-accent preimage: the resolved presence plus RGB. The live
/// read never produces an absent value (conversion failure refuses the
/// category); absence below is only the test seam for the comparison shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccentBaseline {
    pub present: bool,
    pub rgb: Option<(u8, u8, u8)>,
}

/// One running-application entry: PID plus bundle ID. No paths, no names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessBaseline {
    pub pid: i32,
    pub bundle: String,
}

/// One display's native row: Space order plus the current Space. The current
/// is never optional here: a display without a readable current has no
/// goto-back target and the capture refuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeDisplayBaseline {
    pub index: usize,
    pub spaces: Vec<u64>,
    pub current: u64,
}

/// Native topology preimage: separate-Spaces mode plus every display row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeBaseline {
    pub separate_spaces: bool,
    pub displays: Vec<NativeDisplayBaseline>,
}

/// The full live-leg baseline. All categories are required: a missing
/// category aborts the leg, never a partial baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveBaseline {
    pub version: u32,
    pub protected_windows: Vec<ProtectedWindow>,
    pub holders: Vec<HolderBaseline>,
    pub accent: AccentBaseline,
    pub processes: Vec<ProcessBaseline>,
    pub native: NativeBaseline,
}

/// Injected baseline reads. The production implementation
/// ([`LiveBaselineReads`]) performs the real OS reads; tests script every
/// refusal, timeout, and partial capture without touching the desktop.
pub trait BaselineReads {
    fn read_protected_windows(&mut self) -> Result<Vec<ProtectedWindow>, String>;
    fn read_holders(&mut self) -> Result<Vec<HolderBaseline>, String>;
    fn read_accent(&mut self) -> Result<AccentBaseline, String>;
    fn read_processes(&mut self) -> Result<Vec<ProcessBaseline>, String>;
    fn read_native(&mut self) -> Result<NativeBaseline, String>;
}

/// Production reads over the real OS. No decisions: every refusal names its
/// category and the capture maps it to a missing-baseline abort.
pub struct LiveBaselineReads;

impl LiveBaselineReads {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for LiveBaselineReads {
    fn default() -> Self {
        Self::new()
    }
}

impl BaselineReads for LiveBaselineReads {
    fn read_protected_windows(&mut self) -> Result<Vec<ProtectedWindow>, String> {
        // Conservative title-free superset of every window: bundle identity
        // is recorded when readable but never required. An unreadable CG
        // list refuses the whole category (never a fabricated empty
        // baseline); a genuinely empty list is a valid complete-empty
        // preimage. Unknown-bundle rows are treated as protected downstream.
        let windows = crate::ffi::cg_window_list_basic()
            .map_err(|e| format!("protected-window list unreadable: {e}"))?;
        Ok(windows
            .into_iter()
            .map(|window| {
                let bundle = crate::ffi::bundle_id_for_pid(window.pid as i32)
                    .filter(|bundle| !bundle.is_empty());
                let protected = bundle
                    .as_deref()
                    .is_some_and(crate::ownership::is_protected_bundle);
                let ghostty = bundle
                    .as_deref()
                    .is_some_and(crate::scope::is_ghostty_bundle);
                ProtectedWindow {
                    pid: window.pid,
                    wid: window.wid,
                    layer: window.layer,
                    onscreen: window.onscreen,
                    x: window.bounds.x,
                    y: window.bounds.y,
                    w: window.bounds.w,
                    h: window.bounds.h,
                    bundle,
                    bundle_protected: protected,
                    bundle_ghostty: ghostty,
                }
            })
            .collect())
    }

    fn read_holders(&mut self) -> Result<Vec<HolderBaseline>, String> {
        let mut store = crate::desktop_shortcuts::LiveStore::new();
        read_holders_from(&mut store)
    }

    fn read_accent(&mut self) -> Result<AccentBaseline, String> {
        // Fallible native read with an exact schema scope: the preimage is
        // the RESOLVED system accent RGB (`NSColor.controlAccentColor`,
        // always present on a readable AppKit session), never raw
        // preference keys and never a global write. Off the main thread or
        // on conversion failure the accent is genuinely unreadable (never
        // guessed absent): `None` is a conversion failure, NOT absence, so
        // it refuses the category instead of fabricating an absent preimage.
        #[cfg(target_os = "macos")]
        {
            if objc2::MainThreadMarker::new().is_none() {
                return Err("accent read requires the main thread".to_owned());
            }
        }
        match crate::ffi::border_system_accent() {
            Some(theme) => Ok(accent_from(Some(theme))),
            None => Err(
                "system accent conversion unreadable (NSColor.controlAccentColor unresolved); refusing, never guessed absent"
                    .to_owned(),
            ),
        }
    }

    fn read_processes(&mut self) -> Result<Vec<ProcessBaseline>, String> {
        let apps = crate::ffi::running_apps();
        if apps.is_empty() {
            return Err("running-application table unreadable or empty".to_owned());
        }
        Ok(apps
            .into_iter()
            .map(|(pid, bundle)| ProcessBaseline { pid, bundle })
            .collect())
    }

    fn read_native(&mut self) -> Result<NativeBaseline, String> {
        let separate_spaces = crate::ffi::ns_separate_spaces()
            .ok_or_else(|| "separate-Spaces state unreadable".to_owned())?;
        let rows = crate::ffi::sls_display_rows()
            .map_err(|e| format!("display/Space rows unreadable: {e}"))?;
        native_from_rows(separate_spaces, &rows)
    }
}

/// Pure accent mapping over plain data. `None` here is only the test
/// seam for an absent preimage shape (the live read above never produces
/// it: an unreadable conversion refuses the category instead).
#[must_use]
pub fn accent_from(accent: Option<crate::active_border::SystemAccent>) -> AccentBaseline {
    match accent {
        Some(theme) => AccentBaseline {
            present: true,
            rgb: Some(theme.color),
        },
        None => AccentBaseline {
            present: false,
            rgb: None,
        },
    }
}

/// Pure native mapping over plain SLS rows: refuses empty topologies and any
/// display without a readable current (no goto-back target there).
pub fn native_from_rows(
    separate_spaces: bool,
    rows: &[crate::ffi::SlsDisplayRow],
) -> Result<NativeBaseline, String> {
    if rows.is_empty() {
        return Err("no displays in the native topology".to_owned());
    }
    let mut displays = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let Some(current) = row.current else {
            return Err(format!(
                "display {index} has no current Space; refusing before any mutation"
            ));
        };
        displays.push(NativeDisplayBaseline {
            index,
            spaces: row.spaces.clone(),
            current,
        });
    }
    Ok(NativeBaseline {
        separate_spaces,
        displays,
    })
}

/// Holder reads over any shortcut store (live or injected fake): Desktops
/// 1-9, each refusal naming its holder. No writes on any path.
pub fn read_holders_from<S: crate::desktop_shortcuts::DesktopShortcutStore>(
    store: &mut S,
) -> Result<Vec<HolderBaseline>, String> {
    let mut out = Vec::new();
    for desktop in 1..=BASELINE_HOLDER_COUNT {
        let symbolic_id = crate::desktop_shortcuts::desktop_symbolic_id(desktop)
            .ok_or_else(|| format!("desktop {desktop} has no holder id; refusing"))?;
        let entry = store.read(symbolic_id).map_err(|e| {
            format!("shortcut holder for Desktop {desktop} (id {symbolic_id}) unreadable: {e}")
        })?;
        out.push(HolderBaseline {
            symbolic_id,
            desktop,
            enabled: entry.enabled,
            keycode: entry.keycode,
            modifiers: entry.modifiers,
        });
    }
    Ok(out)
}

/// Capture every category in a fixed order. The first missing category wins,
/// so gate ordering is pinned by test. No mutation on any path: pure reads
/// only, persistence is the caller's separate step.
pub fn capture_baseline<R: BaselineReads>(reads: &mut R) -> Result<LiveBaseline, String> {
    let protected_windows = reads
        .read_protected_windows()
        .map_err(|e| format!("baseline protected-windows missing: {e}"))?;
    let holders = reads
        .read_holders()
        .map_err(|e| format!("baseline shortcut-holders missing: {e}"))?;
    let accent = reads
        .read_accent()
        .map_err(|e| format!("baseline accent missing: {e}"))?;
    let processes = reads
        .read_processes()
        .map_err(|e| format!("baseline processes missing: {e}"))?;
    let native = reads
        .read_native()
        .map_err(|e| format!("baseline native-topology missing: {e}"))?;
    let baseline = LiveBaseline {
        version: BASELINE_SCHEMA_VERSION,
        protected_windows,
        holders,
        accent,
        processes,
        native,
    };
    validate_baseline(&baseline)?;
    Ok(baseline)
}

/// Pure baseline validation over plain data: schema version, holder coverage,
/// and a non-empty native topology. Hostile or truncated files refuse before
/// any store touch.
pub fn validate_baseline(baseline: &LiveBaseline) -> Result<(), String> {
    if baseline.version != BASELINE_SCHEMA_VERSION {
        return Err(format!(
            "baseline version {} unsupported (expected {BASELINE_SCHEMA_VERSION})",
            baseline.version
        ));
    }
    if baseline.holders.len() != BASELINE_HOLDER_COUNT as usize {
        return Err(format!(
            "baseline holds {} shortcut holders (expected {BASELINE_HOLDER_COUNT}); refusing a partial baseline",
            baseline.holders.len()
        ));
    }
    if baseline.native.displays.is_empty() {
        return Err("baseline native topology has no displays; refusing".to_owned());
    }
    Ok(())
}

/// Canonical per-user baseline path (`None` without a home: callers refuse,
/// never a temporary fallback).
#[must_use]
pub fn canonical_baseline_path_for(home: Option<&str>) -> Option<PathBuf> {
    crate::desktop_shortcuts::recovery_dir_for(home).map(|dir| dir.join(BASELINE_FILE_NAME))
}

/// Runtime canonical baseline path from `$HOME`.
#[must_use]
pub fn canonical_baseline_path() -> Option<PathBuf> {
    canonical_baseline_path_for(std::env::var("HOME").ok().as_deref())
}

/// Persist a captured baseline BEFORE any input or product startup. Ordering
/// mirrors the preimage publish: the transaction lock serializes full
/// capture/persist/compare sequences across processes, an existing file
/// refuses (the previous leg's receipt is never overwritten: finish it or
/// inspect it first), the write is atomic with no-clobber link semantics,
/// mode 0600, and the parent directory syncs before return. A publish refusal
/// performs zero settings writes after it.
pub fn persist_baseline_to(path: &Path, baseline: &LiveBaseline) -> Result<(), String> {
    validate_baseline(baseline)?;
    let lock_path = crate::desktop_shortcuts::lock_path_for(path);
    let _lock = crate::desktop_shortcuts::lock_leaf_file(&lock_path).map_err(|e| match e {
        crate::desktop_shortcuts::LockRefusal::Held => {
            "baseline persist refused: another leg holds the lock; zero writes performed".to_owned()
        }
        crate::desktop_shortcuts::LockRefusal::Refused(reason) => {
            format!(
                "baseline persist refused: lock unestablishable ({reason}); zero writes performed"
            )
        }
    })?;
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err("baseline path is a symlink; refusing".to_owned());
            }
            return Err(
                "baseline receipt already exists; refusing overwrite (finish or inspect the previous leg first)"
                    .to_owned(),
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("baseline metadata: {e}")),
    }
    let json = serde_json::to_string(baseline).map_err(|e| format!("baseline encode: {e}"))?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("baseline write: {e}"))?;
        file.write_all(json.as_bytes())
            .map_err(|e| format!("baseline write: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("baseline write: {e}"))?;
        drop(file);
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("baseline permissions: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("baseline write: {e}"))?;
        file.write_all(json.as_bytes())
            .map_err(|e| format!("baseline write: {e}"))?;
    }
    match std::fs::hard_link(&tmp, path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&tmp);
            crate::desktop_shortcuts::sync_parent_dir(path)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                return Err(
                    "baseline receipt already exists; refusing overwrite (finish or inspect the previous leg first)"
                        .to_owned(),
                );
            }
            Err(format!("baseline publish: {e}"))
        }
    }
}

/// Load and validate a persisted receipt. Refuses on symlinks, non-regular
/// files, corrupt JSON, and hostile content before any comparison.
pub fn load_baseline_from(path: &Path) -> Result<LiveBaseline, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|_| "no baseline receipt; nothing to compare (capture one first)".to_owned())?;
    if meta.file_type().is_symlink() {
        return Err("baseline receipt is a symlink; refusing".to_owned());
    }
    if !meta.is_file() {
        return Err("baseline receipt is not a regular file; refusing".to_owned());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|_| "baseline receipt unreadable; refusing".to_owned())?;
    let baseline: LiveBaseline =
        serde_json::from_str(&raw).map_err(|_| "baseline receipt corrupt; refusing".to_owned())?;
    validate_baseline(&baseline)?;
    Ok(baseline)
}

/// Remove a receipt after a clean full restoration. A retained receipt (any
/// mismatch or unrestored current) stays for audit and blocks the next
/// persist until inspected.
pub fn remove_baseline_receipt(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| format!("baseline receipt removal: {e}"))
}

/// One teardown mismatch: the category plus the exact residue. Never an
/// automatic overwrite: mismatches abort with this evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineMismatch {
    pub category: String,
    pub detail: String,
}

fn key_windows(
    windows: &[ProtectedWindow],
) -> std::collections::HashMap<(u32, u32), &ProtectedWindow> {
    windows.iter().map(|w| ((w.pid, w.wid), w)).collect()
}

fn protected_pair(processes: &[ProcessBaseline], pid: i32) -> Option<&ProcessBaseline> {
    processes.iter().find(|p| {
        p.pid == pid
            && (crate::ownership::is_protected_bundle(&p.bundle)
                || crate::scope::is_ghostty_bundle(&p.bundle))
    })
}

/// Pure teardown comparison over plain data. `owned_windows` names the
/// exact (pid, wid) pairs this leg created and registered: a registered
/// pair still present at teardown is residue (owned window not closed),
/// while registered pairs are excluded from the foreign-added check (they
/// are accounted, never silently absorbed). Every other drift between the
/// pre-leg baseline and the fresh teardown reads is a mismatch entry:
/// foreign-window add/remove/geometry drift, holder changes (external edits
/// are preserved and reported, never reverted here), accent presence/RGB
/// drift, protected-process add/remove, and native Space-set or current
/// drift. An empty result means exact restoration; a non-empty result aborts
/// the leg with the residue, never a silent pass.
#[must_use]
pub fn compare_teardown(
    before: &LiveBaseline,
    after: &LiveBaseline,
    owned_windows: &[(u32, u32)],
) -> Vec<BaselineMismatch> {
    let mut mismatches = Vec::new();
    let mut push = |category: &str, detail: String| {
        mismatches.push(BaselineMismatch {
            category: category.to_owned(),
            detail,
        });
    };

    // Protected windows: exact per-(pid, wid) join over the conservative
    // superset; add, remove, and any geometry/visibility/layer drift all
    // mismatch. Registered leg-owned pairs are accounted separately (still
    // present means not closed) and never counted as foreign additions.
    {
        let before_map = key_windows(&before.protected_windows);
        let after_map = key_windows(&after.protected_windows);
        let owned = |key: &(u32, u32)| owned_windows.contains(key);
        for (key, window) in &before_map {
            if owned(key) {
                continue;
            }
            match after_map.get(key) {
                None => push(
                    "protected-window",
                    format!(
                        "protected window pid {} wid {} gone after the leg; residue: window absent",
                        key.0, key.1
                    ),
                ),
                Some(after_window) => {
                    if *after_window != *window {
                        push(
                            "protected-window",
                            format!(
                                "protected window pid {} wid {} drifted (layer/onscreen/bounds); residue: geometry not restored",
                                key.0, key.1
                            ),
                        );
                    }
                }
            }
        }
        for key in after_map.keys() {
            if owned(key) {
                // A registered owned pair still present means the leg did
                // not close it: residue, never a silent foreign addition.
                push(
                    "protected-window",
                    format!(
                        "owned window pid {} wid {} still present after the leg; residue: owned window not closed",
                        key.0, key.1
                    ),
                );
            } else if !before_map.contains_key(key) {
                push(
                    "protected-window",
                    format!(
                        "protected window pid {} wid {} appeared during the leg; residue: unexpected window present",
                        key.0, key.1
                    ),
                );
            }
        }
    }

    // Shortcut holders: the current value must still equal the preimage.
    // Anything else is preserved verbatim and reported, never overwritten.
    for before_holder in &before.holders {
        match after
            .holders
            .iter()
            .find(|h| h.symbolic_id == before_holder.symbolic_id)
        {
            None => push(
                "shortcut-holder",
                format!(
                    "holder id {} (Desktop {}) unreadable at teardown; residue: holder state unknown",
                    before_holder.symbolic_id, before_holder.desktop
                ),
            ),
            Some(after_holder) => {
                if after_holder != before_holder {
                    push(
                        "shortcut-holder",
                        format!(
                            "holder id {} (Desktop {}) changed during the leg; residue: external edit preserved, never overwritten",
                            before_holder.symbolic_id, before_holder.desktop
                        ),
                    );
                }
            }
        }
    }

    // Accent: presence and RGB must match the preimage.
    if after.accent != before.accent {
        push(
            "accent",
            "system accent changed during the leg; residue: accent not restored".to_owned(),
        );
    }

    // Processes: every protected/Ghostty (pid, bundle) pair observed before
    // the leg must still be present; new protected PIDs are residue too.
    // Unrelated churn is ignored: only protected identities are invariant.
    {
        for process in &before.processes {
            if !crate::ownership::is_protected_bundle(&process.bundle)
                && !crate::scope::is_ghostty_bundle(&process.bundle)
            {
                continue;
            }
            match protected_pair(&after.processes, process.pid) {
                Some(after_process) if after_process.bundle == process.bundle => {}
                _ => push(
                    "process",
                    format!(
                        "protected process pid {} ({}) gone or rebound after the leg; residue: process identity changed",
                        process.pid, process.bundle
                    ),
                ),
            }
        }
        for process in &after.processes {
            if !crate::ownership::is_protected_bundle(&process.bundle)
                && !crate::scope::is_ghostty_bundle(&process.bundle)
            {
                continue;
            }
            if protected_pair(&before.processes, process.pid).is_none() {
                push(
                    "process",
                    format!(
                        "protected process pid {} ({}) appeared during the leg; residue: unexpected protected process",
                        process.pid, process.bundle
                    ),
                );
            }
        }
    }

    // Native topology: same display count and identity order, exact Space
    // sets, and currents equal to the starting currents (the leg restores
    // before comparing; anything else is residue).
    if after.native.displays.len() != before.native.displays.len() {
        push(
            "native-topology",
            format!(
                "display count changed ({} -> {}); residue: topology not restored",
                before.native.displays.len(),
                after.native.displays.len()
            ),
        );
    } else {
        for (before_display, after_display) in before
            .native
            .displays
            .iter()
            .zip(after.native.displays.iter())
        {
            if after_display.spaces != before_display.spaces {
                push(
                    "native-topology",
                    format!(
                        "display {} Space set changed; residue: Spaces created or destroyed",
                        before_display.index
                    ),
                );
            }
            if after_display.current != before_display.current {
                push(
                    "native-topology",
                    format!(
                        "display {} current Space {} != starting {}; residue: starting Space not restored",
                        before_display.index, after_display.current, before_display.current
                    ),
                );
            }
        }
    }
    if after.native.separate_spaces != before.native.separate_spaces {
        push(
            "native-topology",
            "separate-Spaces mode changed during the leg; residue: mode not restored".to_owned(),
        );
    }

    mismatches
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn holder(desktop: u32, enabled: bool) -> HolderBaseline {
        HolderBaseline {
            symbolic_id: crate::desktop_shortcuts::desktop_symbolic_id(desktop).expect("holder id"),
            desktop,
            enabled,
            keycode: crate::desktop_shortcuts::desktop_digit_keycode(desktop).unwrap_or(0),
            modifiers: crate::desktop_shortcuts::CG_EVENT_CTRL_BIT,
        }
    }

    fn good_baseline() -> LiveBaseline {
        LiveBaseline {
            version: BASELINE_SCHEMA_VERSION,
            protected_windows: vec![ProtectedWindow {
                pid: 111,
                wid: 7,
                layer: 0,
                onscreen: true,
                x: 10.0,
                y: 20.0,
                w: 800.0,
                h: 600.0,
                bundle: Some("com.example.ghostty-like".to_owned()),
                bundle_protected: false,
                bundle_ghostty: true,
            }],
            holders: (1..=BASELINE_HOLDER_COUNT)
                .map(|d| holder(d, d != 3))
                .collect(),
            accent: AccentBaseline {
                present: true,
                rgb: Some((42, 130, 218)),
            },
            processes: vec![
                ProcessBaseline {
                    pid: 111,
                    bundle: "com.example.ghostty-like".to_owned(),
                },
                ProcessBaseline {
                    pid: 222,
                    bundle: "com.apple.Terminal".to_owned(),
                },
            ],
            native: NativeBaseline {
                separate_spaces: true,
                displays: vec![
                    NativeDisplayBaseline {
                        index: 0,
                        spaces: vec![11, 22],
                        current: 11,
                    },
                    NativeDisplayBaseline {
                        index: 1,
                        spaces: vec![33],
                        current: 33,
                    },
                ],
            },
        }
    }

    /// Scripted reads: each category returns its canned value or a refusal.
    /// Call counters pin the capture order (protected first, native last).
    struct ScriptReads {
        protected: Result<Vec<ProtectedWindow>, String>,
        holders: Result<Vec<HolderBaseline>, String>,
        accent: Result<AccentBaseline, String>,
        processes: Result<Vec<ProcessBaseline>, String>,
        native: Result<NativeBaseline, String>,
        order: RefCell<Vec<&'static str>>,
    }

    impl ScriptReads {
        fn passing() -> Self {
            let good = good_baseline();
            Self {
                protected: Ok(good.protected_windows.clone()),
                holders: Ok(good.holders.clone()),
                accent: Ok(good.accent),
                processes: Ok(good.processes.clone()),
                native: Ok(good.native.clone()),
                order: RefCell::new(Vec::new()),
            }
        }
    }

    impl BaselineReads for ScriptReads {
        fn read_protected_windows(&mut self) -> Result<Vec<ProtectedWindow>, String> {
            self.order.borrow_mut().push("protected");
            self.protected.clone()
        }
        fn read_holders(&mut self) -> Result<Vec<HolderBaseline>, String> {
            self.order.borrow_mut().push("holders");
            self.holders.clone()
        }
        fn read_accent(&mut self) -> Result<AccentBaseline, String> {
            self.order.borrow_mut().push("accent");
            self.accent.clone()
        }
        fn read_processes(&mut self) -> Result<Vec<ProcessBaseline>, String> {
            self.order.borrow_mut().push("processes");
            self.processes.clone()
        }
        fn read_native(&mut self) -> Result<NativeBaseline, String> {
            self.order.borrow_mut().push("native");
            self.native.clone()
        }
    }

    #[test]
    fn capture_orders_gates_and_names_missing_category() {
        let mut reads = ScriptReads::passing();
        let baseline = capture_baseline(&mut reads).expect("full capture passes");
        assert_eq!(baseline.holders.len(), BASELINE_HOLDER_COUNT as usize);
        assert_eq!(
            *reads.order.borrow(),
            vec!["protected", "holders", "accent", "processes", "native"]
        );
        // Each missing category aborts before any later read is judged: the
        // error names the category, and earlier reads still ran (no skip).
        let mut missing_protected = ScriptReads::passing();
        missing_protected.protected = Err("CG refused".to_owned());
        let err = capture_baseline(&mut missing_protected).unwrap_err();
        assert!(err.contains("protected-windows"), "unexpected: {err}");
        assert_eq!(*missing_protected.order.borrow(), vec!["protected"]);

        let mut missing_holders = ScriptReads::passing();
        missing_holders.holders = Err("holder 119 unreadable".to_owned());
        let err = capture_baseline(&mut missing_holders).unwrap_err();
        assert!(err.contains("shortcut-holders"), "unexpected: {err}");

        let mut missing_accent = ScriptReads::passing();
        missing_accent.accent = Err("accent pipe wedged".to_owned());
        let err = capture_baseline(&mut missing_accent).unwrap_err();
        assert!(err.contains("accent"), "unexpected: {err}");

        let mut missing_processes = ScriptReads::passing();
        missing_processes.processes = Err("table empty".to_owned());
        let err = capture_baseline(&mut missing_processes).unwrap_err();
        assert!(err.contains("processes"), "unexpected: {err}");

        let mut missing_native = ScriptReads::passing();
        missing_native.native = Err("no current".to_owned());
        let err = capture_baseline(&mut missing_native).unwrap_err();
        assert!(err.contains("native-topology"), "unexpected: {err}");
        // First failure wins: holders missing beats a later native refusal.
        let mut two = ScriptReads::passing();
        two.holders = Err("holder gone".to_owned());
        two.native = Err("native gone".to_owned());
        let err = capture_baseline(&mut two).unwrap_err();
        assert!(err.contains("shortcut-holders"), "holders must win: {err}");
    }

    #[test]
    fn native_mapping_refuses_unknown_currents_and_empty_rows() {
        let rows = vec![
            crate::ffi::SlsDisplayRow {
                identifier: Some("display-a".to_owned()),
                spaces: vec![11, 22],
                current: Some(11),
            },
            crate::ffi::SlsDisplayRow {
                identifier: Some("display-b".to_owned()),
                spaces: vec![33],
                current: Some(33),
            },
        ];
        let native = native_from_rows(true, &rows).expect("readable rows map");
        assert_eq!(native.displays.len(), 2);
        assert_eq!(native.displays[0].current, 11);
        assert!(native_from_rows(true, &[]).is_err());
        let no_current = vec![crate::ffi::SlsDisplayRow {
            identifier: Some("display-a".to_owned()),
            spaces: vec![11],
            current: None,
        }];
        let err = native_from_rows(true, &no_current).unwrap_err();
        assert!(err.contains("no current"), "unexpected: {err}");
    }

    #[test]
    fn accent_absence_is_a_valid_preimage() {
        // Pure mapping seam only: the live read never produces absence (an
        // unreadable conversion refuses the category instead).
        assert_eq!(
            accent_from(None),
            AccentBaseline {
                present: false,
                rgb: None
            }
        );
        let present = accent_from(Some(crate::active_border::SystemAccent {
            color: (1, 2, 3),
            alpha: 255,
        }));
        assert_eq!(
            present,
            AccentBaseline {
                present: true,
                rgb: Some((1, 2, 3))
            }
        );
    }

    #[test]
    fn accent_conversion_unavailable_fails_the_category() {
        // `NSColor.controlAccentColor` always resolves on a readable
        // session: a `None` conversion is a failure, never absence. The
        // live read refuses the category (off-main on macOS, stubbed `None`
        // elsewhere) instead of fabricating an absent preimage.
        let err = LiveBaselineReads::new().read_accent().unwrap_err();
        assert!(
            err.contains("main thread") || err.contains("unreadable"),
            "conversion failure must fail the category, never absence: {err}"
        );
    }

    #[test]
    fn holder_reads_cover_nine_desktops_and_name_refusals() {
        let mut store = crate::desktop_shortcuts::FakeStore::new();
        for desktop in 1..=BASELINE_HOLDER_COUNT {
            let id = crate::desktop_shortcuts::desktop_symbolic_id(desktop).expect("id");
            store = store.with_entry(
                id,
                crate::desktop_shortcuts::ShortcutEntry {
                    enabled: desktop != 2,
                    keycode: 0x12,
                    modifiers: crate::desktop_shortcuts::CG_EVENT_CTRL_BIT,
                },
            );
        }
        let holders = read_holders_from(&mut store).expect("nine holders read");
        assert_eq!(holders.len(), 9);
        assert!(!holders[1].enabled);
        // An unreadable holder refuses naming its desktop, before any write.
        let mut failing = crate::desktop_shortcuts::FakeStore::new();
        for desktop in 1..=BASELINE_HOLDER_COUNT {
            let id = crate::desktop_shortcuts::desktop_symbolic_id(desktop).expect("id");
            failing = failing.with_entry(
                id,
                crate::desktop_shortcuts::ShortcutEntry {
                    enabled: true,
                    keycode: 0x12,
                    modifiers: crate::desktop_shortcuts::CG_EVENT_CTRL_BIT,
                },
            );
        }
        failing
            .fail_reads
            .insert(crate::desktop_shortcuts::desktop_symbolic_id(4).expect("id"));
        let err = read_holders_from(&mut failing).unwrap_err();
        assert!(err.contains("Desktop 4"), "unexpected: {err}");
        assert_eq!(failing.writes, 0, "refusal performs zero writes");
    }

    #[test]
    fn persist_refuses_overwrite_and_loads_exact_receipt() {
        let dir = std::env::temp_dir().join(format!("legbase-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test dir creates");
        // Private-dir hardening matches the preimage store (0700 owned).
        crate::desktop_shortcuts::ensure_private_dir(&dir).expect("private dir");
        let path = dir.join("receipt.json");
        let baseline = good_baseline();
        persist_baseline_to(&path, &baseline).expect("fresh persist passes");
        // A second persist refuses: the previous receipt is never overwritten.
        let err = persist_baseline_to(&path, &baseline).unwrap_err();
        assert!(err.contains("already exists"), "unexpected: {err}");
        let loaded = load_baseline_from(&path).expect("receipt loads");
        assert_eq!(loaded, baseline);
        // Corrupt and hostile files refuse before any comparison.
        std::fs::write(&path, "{not json").expect("corrupt writes");
        assert!(load_baseline_from(&path).is_err());
        let hostile = LiveBaseline {
            version: 999,
            ..baseline.clone()
        };
        let hostile_json = serde_json::to_string(&hostile).expect("hostile encodes");
        std::fs::write(&path, hostile_json).expect("hostile writes");
        assert!(load_baseline_from(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_failure_aborts_before_any_input() {
        // An uncreatable path (missing parent, never chmodded into place)
        // refuses the persist with zero reads judged after it.
        let dir = std::env::temp_dir().join(format!("legbase-nope-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("missing-parent").join("receipt.json");
        let baseline = good_baseline();
        let err = persist_baseline_to(&path, &baseline).unwrap_err();
        assert!(
            err.contains("lock") || err.contains("write") || err.contains("metadata"),
            "unexpected: {err}"
        );
        assert!(!path.exists());
    }

    #[test]
    fn teardown_compare_passes_exact_and_reports_every_drift() {
        let before = good_baseline();
        assert!(compare_teardown(&before, &before, &[]).is_empty());
        // Protected-window geometry drift mismatches (never auto-restored).
        let mut drifted = before.clone();
        drifted.protected_windows[0].w = 801.0;
        let mismatches = compare_teardown(&before, &drifted, &[]);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].category, "protected-window");
        // A removed and an added protected window each mismatch.
        let mut removed = before.clone();
        removed.protected_windows.clear();
        assert!(
            compare_teardown(&before, &removed, &[])
                .iter()
                .any(|m| m.category == "protected-window")
        );
        let mut added = before.clone();
        let mut extra = added.protected_windows[0].clone();
        extra.wid = 99;
        added.protected_windows.push(extra);
        assert!(
            compare_teardown(&before, &added, &[])
                .iter()
                .any(|m| m.category == "protected-window")
        );
        // Holder changes mismatch and are never overwritten here.
        let mut holder_changed = before.clone();
        holder_changed.holders[0].enabled = !holder_changed.holders[0].enabled;
        let mismatches = compare_teardown(&before, &holder_changed, &[]);
        assert!(mismatches.iter().any(|m| m.category == "shortcut-holder"));
        assert!(
            mismatches[0].detail.contains("never overwritten")
                || mismatches.iter().any(|m| m.detail.contains("preserved"))
        );
        // Accent drift mismatches.
        let mut accent_changed = before.clone();
        accent_changed.accent = AccentBaseline {
            present: false,
            rgb: None,
        };
        assert!(
            compare_teardown(&before, &accent_changed, &[])
                .iter()
                .any(|m| m.category == "accent")
        );
        // Protected-process loss mismatches; unrelated churn is ignored.
        let mut process_gone = before.clone();
        process_gone.processes.retain(|p| p.pid != 222);
        assert!(
            compare_teardown(&before, &process_gone, &[])
                .iter()
                .any(|m| m.category == "process")
        );
        let mut churn = before.clone();
        churn.processes.push(ProcessBaseline {
            pid: 4242,
            bundle: "com.example.unrelated".to_owned(),
        });
        assert!(compare_teardown(&before, &churn, &[]).is_empty());
        // Native current and Space-set drift mismatch.
        let mut current_moved = before.clone();
        current_moved.native.displays[0].current = 22;
        let mismatches = compare_teardown(&before, &current_moved, &[]);
        assert!(mismatches.iter().any(|m| m.category == "native-topology"));
        assert!(mismatches.iter().any(|m| m.detail.contains("not restored")));
        let mut space_added = before.clone();
        space_added.native.displays[0].spaces.push(99);
        assert!(
            compare_teardown(&before, &space_added, &[])
                .iter()
                .any(|m| m.category == "native-topology")
        );
        let mut mode_flipped = before.clone();
        mode_flipped.native.separate_spaces = false;
        assert!(
            compare_teardown(&before, &mode_flipped, &[])
                .iter()
                .any(|m| m.category == "native-topology")
        );
    }

    #[test]
    fn teardown_accounts_registered_owned_windows() {
        let before = good_baseline();
        // A registered owned pair absent at teardown is fine (closed as
        // required); still present is residue (not closed).
        let mut closed = before.clone();
        closed.protected_windows.clear();
        assert!(
            compare_teardown(&before, &closed, &[(111, 7)])
                .iter()
                .all(|m| m.category != "protected-window")
        );
        let lingering = before.clone();
        let mismatches = compare_teardown(&before, &lingering, &[(111, 7)]);
        assert!(
            mismatches.iter().any(|m| m.detail.contains("not closed")),
            "unexpected: {mismatches:?}"
        );
        // An unregistered addition still mismatches even when an owned
        // pair is registered alongside it.
        let mut foreign = before.clone();
        let mut extra = foreign.protected_windows[0].clone();
        extra.wid = 99;
        foreign.protected_windows.push(extra);
        let mismatches = compare_teardown(&before, &foreign, &[(111, 7)]);
        assert!(
            mismatches
                .iter()
                .any(|m| m.detail.contains("unexpected window present")),
            "unexpected: {mismatches:?}"
        );
    }

    #[test]
    fn unknown_bundle_windows_are_retained_not_omitted() {
        // An unreadable owner bundle stays in the baseline (conservatively
        // protected) and compares exactly like any other row.
        let mut unknown = good_baseline();
        unknown.protected_windows[0].bundle = None;
        unknown.protected_windows[0].bundle_protected = false;
        unknown.protected_windows[0].bundle_ghostty = false;
        assert!(compare_teardown(&unknown, &unknown, &[]).is_empty());
        let mut drifted = unknown.clone();
        drifted.protected_windows[0].x = 11.0;
        assert!(
            compare_teardown(&unknown, &drifted, &[])
                .iter()
                .any(|m| m.category == "protected-window")
        );
    }

    #[test]
    fn baseline_reports_carry_ids_only() {
        // Schema carries IDs, geometry, and state only: no titles, paths,
        // nonces, executables, or requirement text.
        let flat = serde_json::to_string(&good_baseline()).expect("baseline flattens");
        for leaked in [
            "title",
            "nonce",
            "executable",
            "requirement",
            "signer",
            "socket",
            "suffix",
        ] {
            assert!(!flat.contains(leaked), "schema leaks {leaked}");
        }
    }
}
