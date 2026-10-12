#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};

pub const LEDGER_SCHEMA_VERSION: u32 = 4;

/// Project-specific window-lifetime property backing product (ordinary-app)
/// hide claims. Distinct from the helper `OmniTilerLifetime` property so
/// helper-only gates never mistake a product nonce for helper ownership.
pub const PRODUCT_CLAIM_PROP: &str = "OmniTilerProductClaim";

/// Project-specific window-lifetime property backing visible workspace
/// membership. Stamped once at admission with a random tag (never trusted
/// across runs, never removed: the property dies with the window). A
/// same-process HWND reuse starts without it, so the stored tag fails closed
/// exactly where HWND/PID/creation all still agree. Distinct from
/// [`PRODUCT_CLAIM_PROP`] (hidden claims) and the helper lifetime property so
/// the three domains never mistake each other's tags.
pub const MEMBER_TAG_PROP: &str = "OmniTilerMember";

/// Sticky-float marker (1 = pre-sticky tiled, 2 = pre-sticky float). Dies
/// with the window; removed only by sticky-off, kept across adoption so
/// later restarts still see it. The legacy `OmniTilerSticky` name is
/// retired and never read.
pub const STICKY_PROP: &str = "OmniTiler.Sticky.v1";

/// Intentional-float marker (value 1). Written after native success;
/// cleared on settled unfloat. Missing means no intent; unreadable or
/// mismatch is diagnosed with no intent. Classification only, never write
/// or recovery authority. Sticky wins when both markers read valid.
pub const FLOAT_INTENT_PROP: &str = "OmniTiler.FloatIntent.v1";

/// Reserved fixed-window tile-override marker (value 1) for item 13 (D7).
/// Item 8 reserves the constant, codec, and native ops only: no admission
/// path reads it here and it never floats a window.
pub const TILE_OVERRIDE_PROP: &str = "OmniTiler.TileOverride.v1";

/// Ownership domain of one hidden-window claim. Helper claims use the owned
/// test-window lifetime property and helper-only gates; product claims use the
/// product nonce property on ordinary windows. Serde default is helper so
/// ledgers written before product work (v1/v2, no `kind` field) still parse as
/// helper-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WindowClaimKind {
    #[default]
    Helper,
    Product,
}

#[must_use]
pub fn default_claim_kind() -> WindowClaimKind {
    WindowClaimKind::Helper
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub process_creation: String,
    pub user_sid: String,
    pub session_id: u32,
    pub exe_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowIdentity {
    pub hwnd: u64,
    pub process: ProcessIdentity,
    pub tag: String,
    #[serde(default = "default_claim_kind")]
    pub kind: WindowClaimKind,
    /// Native show-state preimage captured fresh before hide (no geometry):
    /// whether the window was iconic (minimized) and whether it was
    /// maximized. Missing on ledgers written before show-state capture means
    /// a normal restore. Drives the nonactivating reveal choice so retained
    /// minimized/maximized members return to their pre-hide state even when
    /// only the durable ledger (watcher, standalone restore, graceful
    /// teardown) is available.
    #[serde(default)]
    pub show: WindowShowState,
}

/// Minimal show-state preimage for one product claim: iconic and maximized at
/// hide time. No geometry, no placement, no style bits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowShowState {
    #[serde(default)]
    pub minimized: bool,
    #[serde(default)]
    pub maximized: bool,
}

/// Closed reveal vocabulary for one show-state preimage. Minimized takes
/// precedence: a preimage claiming both (corrupt or raced) still restores
/// minimized rather than unminimizing. Maximized and normal both reveal with
/// the nonactivating show that preserves native placement; only minimized
/// needs its own command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductShowRestore {
    ShowNormal,
    ShowMinimized,
    ShowMaximized,
}

/// Decide the reveal of one show-state preimage. Identity fencing and drift
/// handling (already-visible, retired) stay with the caller; this maps only
/// the durable pre-hide state to its restore intent.
#[must_use]
pub const fn product_show_restore(show: WindowShowState) -> ProductShowRestore {
    if show.minimized {
        ProductShowRestore::ShowMinimized
    } else if show.maximized {
        ProductShowRestore::ShowMaximized
    } else {
        ProductShowRestore::ShowNormal
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedWindow {
    pub identity: WindowIdentity,
    pub visible: bool,
}

/// Session-only mouse-Snap (`SPI_SETWINARRANGING`) preimage persisted in the
/// owner ledger before the first setter (write-before-effect). `original` is
/// the exact live boolean captured with `SPI_GETWINARRANGING`; `owned` is an
/// intent-first recovery claim persisted before the setter runs, not only a
/// verified effect. A crash past the intent commit still recovers: live
/// `TRUE` then reads as drift-preserve, live `FALSE` as ours to restore, and
/// a failed read retains the claim for a later independent restore. A
/// readback mismatch later downgrades the claim to `owned: false`; an
/// unverified setter error retains the intent claim. When `original` was
/// already false no mutation is owned and teardown must not write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseSnapPreimage {
    pub original: bool,
    pub owned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryLedger {
    pub v: u32,
    pub owner: ProcessIdentity,
    pub windows: Vec<WindowIdentity>,
    /// Optional so ledgers written before mouse-Snap prevention still parse
    /// (missing means no Snap work). Writes use schema v4; older v1/v2/v3-only
    /// readers refuse v4 outright, so they can never silently ignore the field
    /// or a product claim, restore windows, and delete the ledger while
    /// leaving the setting off. The same version gate protects the v4
    /// show-state preimage: a v3-only reader refuses v4 instead of ignoring
    /// the preimage and unminimizing retained windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse_snap: Option<MouseSnapPreimage>,
}

/// True when the ledger carries an intent-first ownership claim over an
/// originally-true setting: the only state that authorizes restoration. This
/// includes unverified setter attempts retained for later independent
/// restore; the teardown decision still requires the live value to equal our
/// `FALSE` effect before any write.
#[must_use]
pub const fn mouse_snap_owned(preimage: Option<MouseSnapPreimage>) -> bool {
    match preimage {
        Some(record) => record.original && record.owned,
        None => false,
    }
}

/// Closed teardown vocabulary for one Snap preimage against a fresh live
/// read. `live` is `None` when the `SPI_GETWINARRANGING` read itself failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseSnapRestoreDecision {
    /// Live is still our verified `FALSE`: restore the exact original `TRUE`.
    Restore,
    /// Live drifted to `TRUE` (foreign change): preserve it, never overwrite
    /// to force restoration.
    PreserveDrift,
    /// No owned mutation (missing preimage, originally false, or an
    /// unverified attempt): nothing to write.
    NoOwnedMutation,
    /// Live read failed: retain the ledger for a later independent restore,
    /// never clean recovery evidence while uncertain.
    UncertainRetain,
}

/// Decide the teardown of one Snap preimage. Restoration requires the strict
/// conjunction of an owned mutation and a live value still equal to our
/// `FALSE` effect; every other combination preserves or retains.
#[must_use]
pub const fn mouse_snap_restore_decision(
    preimage: Option<MouseSnapPreimage>,
    live: Option<bool>,
) -> MouseSnapRestoreDecision {
    if !mouse_snap_owned(preimage) {
        return MouseSnapRestoreDecision::NoOwnedMutation;
    }
    match live {
        Some(false) => MouseSnapRestoreDecision::Restore,
        Some(true) => MouseSnapRestoreDecision::PreserveDrift,
        None => MouseSnapRestoreDecision::UncertainRetain,
    }
}

/// Closed setup vocabulary for one exact `SPI_GETWINARRANGING` preimage read.
/// `live` is `None` when the read itself failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseSnapSetupDecision {
    /// Originally true: persist the preimage, then set `FALSE` with readback.
    Disable,
    /// Originally false: record the preimage, claim no mutation, write nothing.
    AlreadyOff,
    /// Preimage read failed: write nothing, degrade tiling-only, retry at a
    /// later meaningful opportunity.
    UnknownRetain,
}

/// Decide the setup of one Snap preimage read. `None` (failed read) never
/// authorizes a setter.
#[must_use]
pub const fn mouse_snap_setup_decision(live: Option<bool>) -> MouseSnapSetupDecision {
    match live {
        Some(true) => MouseSnapSetupDecision::Disable,
        Some(false) => MouseSnapSetupDecision::AlreadyOff,
        None => MouseSnapSetupDecision::UnknownRetain,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerError {
    Malformed,
    UnsupportedVersion,
    DuplicateWindow,
    OwnerWindowMismatch,
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed => f.write_str("malformed"),
            Self::UnsupportedVersion => f.write_str("unsupported-version"),
            Self::DuplicateWindow => f.write_str("duplicate-window"),
            Self::OwnerWindowMismatch => f.write_str("owner-window-mismatch"),
        }
    }
}

impl std::error::Error for LedgerError {}

pub fn validate_ledger(ledger: &RecoveryLedger) -> Result<(), LedgerError> {
    match ledger.v {
        // v1 predates Snap work: accept only with no Snap claim. A v1 tag
        // carrying a Snap field is version confusion and must not parse as
        // owner evidence.
        1 => {
            if ledger.mouse_snap.is_some() {
                return Err(LedgerError::UnsupportedVersion);
            }
            if ledger
                .windows
                .iter()
                .any(|w| w.kind != WindowClaimKind::Helper)
            {
                return Err(LedgerError::UnsupportedVersion);
            }
            if ledger
                .windows
                .iter()
                .any(|w| w.show != WindowShowState::default())
            {
                return Err(LedgerError::UnsupportedVersion);
            }
        }
        // v2 helper-only writes. A v2 tag carrying a product claim is version
        // confusion and must not parse as helper evidence.
        2 => {
            if ledger
                .windows
                .iter()
                .any(|w| w.kind != WindowClaimKind::Helper)
            {
                return Err(LedgerError::UnsupportedVersion);
            }
            if ledger
                .windows
                .iter()
                .any(|w| w.show != WindowShowState::default())
            {
                return Err(LedgerError::UnsupportedVersion);
            }
        }
        // Previous product writes without a show-state preimage: missing
        // `show` parses as a normal restore. Older v1/v2-only readers refuse
        // v3 outright via the version gate, so they can never silently ignore
        // a product claim and delete the ledger.
        3 => {}
        // Current writes: helper and product claims plus the show-state
        // preimage. Older v3-only readers refuse v4 outright via the version
        // gate, so they can never silently ignore the preimage, restore
        // retained windows unminimized, and delete the ledger.
        4 => {}
        _ => return Err(LedgerError::UnsupportedVersion),
    }
    for window in &ledger.windows {
        if window.process.user_sid != ledger.owner.user_sid
            || window.process.session_id != ledger.owner.session_id
        {
            return Err(LedgerError::OwnerWindowMismatch);
        }
        // v3/v4 binds the window-lifetime nonce: a product tag must be a nonzero
        // 16-digit lowercase hex token, otherwise it cannot distinguish
        // same-process HWND reuse and must never commit.
        if ledger.v == 3 || ledger.v == 4 {
            if window.kind == WindowClaimKind::Product {
                if !valid_claim_tag(&window.tag) {
                    return Err(LedgerError::Malformed);
                }
            } else if window.tag.is_empty() {
                return Err(LedgerError::Malformed);
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    for window in &ledger.windows {
        if !seen.insert(window.hwnd) {
            return Err(LedgerError::DuplicateWindow);
        }
    }
    Ok(())
}

pub fn parse_ledger(json: &str) -> Result<RecoveryLedger, LedgerError> {
    let ledger: RecoveryLedger = serde_json::from_str(json).map_err(|_| LedgerError::Malformed)?;
    validate_ledger(&ledger)?;
    Ok(ledger)
}

/// Opaque window-lifetime nonce format: 16 lowercase hex digits, nonzero.
/// Shared by helper lifetime tokens and product claim tags; the property name
/// (helper vs [`PRODUCT_CLAIM_PROP`]) separates the domains, never the format.
#[must_use]
pub fn valid_claim_tag(tag: &str) -> bool {
    if tag.len() != 16 {
        return false;
    }
    if !tag.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    if tag.bytes().all(|b| b == b'0') {
        return false;
    }
    tag.bytes().all(|b| !b.is_ascii_uppercase())
}

/// Portable per-hide nonce: time nanos folded with pid and an atomic counter
/// via FNV-1a, rendered as 16 lowercase hex, never zero. Uniqueness per hide
/// (not secrecy) is the binding property: a recycled HWND in the same process
/// starts without our window-lifetime property, so its missing/different tag
/// fails closed. Retries once on the all-zero corner.
#[must_use]
pub fn generate_claim_tag(pid: u32) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15);
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut hash: u64 = 0xcbf29ce484222325;
    for word in [
        nanos,
        u64::from(pid),
        count.wrapping_mul(0x9e3779b97f4a7c15),
        0x241ed0d125c3d4b,
    ] {
        hash ^= word;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == 0 {
        hash = 0x9e3779b97f4a7c15;
    }
    format!("{hash:016x}")
}

/// Exact-owner binding for the crash watcher: only a ledger owned by the exact
/// captured owner (full process tuple) may be restored by that watcher. A
/// late watcher observing a replacement owner must exit without touching
/// state. Used by the under-lease restore guard.
#[must_use]
pub fn watcher_may_restore(captured: &ProcessIdentity, ledger_owner: &ProcessIdentity) -> bool {
    captured == ledger_owner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefuseReason {
    MissingObservation,
    AmbiguousObservation,
    ProcessMismatch,
    TagMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreDecision {
    Reveal { hwnd: u64 },
    SkipAlreadyVisible { hwnd: u64 },
    Refuse { reason: RefuseReason },
}

#[must_use]
pub fn restore_eligibility(
    record: &WindowIdentity,
    observations: &[ObservedWindow],
) -> RestoreDecision {
    let mut matched: Option<&ObservedWindow> = None;
    for observed in observations {
        if observed.identity.hwnd == record.hwnd {
            if matched.is_some() {
                return RestoreDecision::Refuse {
                    reason: RefuseReason::AmbiguousObservation,
                };
            }
            matched = Some(observed);
        }
    }
    let Some(observed) = matched else {
        return RestoreDecision::Refuse {
            reason: RefuseReason::MissingObservation,
        };
    };
    if observed.identity.process != record.process {
        return RestoreDecision::Refuse {
            reason: RefuseReason::ProcessMismatch,
        };
    }
    if observed.identity.tag != record.tag {
        return RestoreDecision::Refuse {
            reason: RefuseReason::TagMismatch,
        };
    }
    if observed.visible {
        RestoreDecision::SkipAlreadyVisible { hwnd: record.hwnd }
    } else {
        RestoreDecision::Reveal { hwnd: record.hwnd }
    }
}
