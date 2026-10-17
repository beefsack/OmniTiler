#![forbid(unsafe_code)]
//! Native read-only gaming-coexistence probes (`cfg(windows)` only).
//!
//! Mechanism: one bounded `HKCU` string read for the undocumented Xbox
//! home-app mapping plus one count-only
//! [`GetPackagesByPackageFamily`](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getpackagesbypackagefamily)
//! query for the exact Game Bar family (current user, no names fetched, no
//! package inventory). Both run on the [`crate::gaming`] worker thread only,
//! never on tile/input loops; the wide-string cap bounds every read.
//!
//! The raw registry payload is classified locally and never returned or
//! logged: callers and logs see only the categorical facts in
//! [`crate::gaming::GamingSnapshot`]. No writes, no uninstall, no Settings
//! navigation here (the allowlisted URIs live in [`crate::gaming`]; Settings
//! UI owns their presentation).

use crate::gaming::{
    GAMEBAR_PACKAGE_FAMILY, GamingSnapshot, XboxConfig, XboxOutcome, XboxRead,
    classify_package_query, classify_xbox, classify_xbox_bytes, is_allowed_navigation_uri,
};

/// Read-only Xbox probe: open the per-user key read-only, fetch the home-app
/// string within the cap, classify the exact recipe mapping, and always close
/// the key. Absent, unreadable, or unrecognised stays unknown.
#[must_use]
pub fn probe_xbox() -> (XboxConfig, XboxOutcome) {
    let read = match crate::ffi::gaming::read_xbox_value(
        crate::gaming::XBOX_CONFIG_SUBKEY,
        crate::gaming::XBOX_CONFIG_VALUE,
    ) {
        crate::ffi::gaming::RawXbox::Absent => XboxRead::Absent,
        crate::ffi::gaming::RawXbox::Unreadable => XboxRead::Unreadable,
        crate::ffi::gaming::RawXbox::Bytes(raw) => classify_xbox_bytes(&raw),
    };
    classify_xbox(&read)
}

/// Read-only Game Bar probe: count-only family query for the current user.
/// A counted match reports registered; a successful empty query reports
/// absent; any query failure reports unknown (never absent). No full names
/// are fetched, so no inventory exists to log.
#[must_use]
pub fn probe_gamebar() -> (crate::gaming::GameBarPackage, crate::gaming::GameBarOutcome) {
    let raw = crate::ffi::gaming::query_package_family(GAMEBAR_PACKAGE_FAMILY);
    classify_package_query(raw.ok, raw.count)
}

/// Both probes in one worker pass. Failures keep conservative unknown facts.
#[must_use]
pub fn probe_all() -> GamingSnapshot {
    let (xbox, xbox_outcome) = probe_xbox();
    let (gamebar, gamebar_outcome) = probe_gamebar();
    GamingSnapshot {
        xbox,
        xbox_outcome,
        gamebar,
        gamebar_outcome,
        generation: 0,
    }
}

/// Open one allowlisted Settings URI through the shell. Refuses
/// anything off [`is_allowed_navigation_uri`] before touching Win32; the
/// launch result never claims remediation.
#[must_use]
pub fn open_navigation_uri(uri: &str) -> bool {
    if !is_allowed_navigation_uri(uri) {
        return false;
    }
    crate::ffi::gaming::launch_uri(uri)
}
