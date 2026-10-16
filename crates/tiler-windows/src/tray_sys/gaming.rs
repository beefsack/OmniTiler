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

/// Wide (NUL-terminated) encoding for one Win32 string parameter.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Upper bound for the home-app string read (UTF-16 units, including NUL).
/// The on AUMID is far shorter; anything longer is unrecognised, never
/// truncated into a match.
const XBOX_VALUE_CAP_UNITS: usize = 256;

/// Read-only Xbox probe: open the per-user key read-only, fetch the home-app
/// string within the cap, classify the exact recipe mapping, and always close
/// the key. Absent, unreadable, or unrecognised stays unknown.
#[must_use]
pub fn probe_xbox() -> (XboxConfig, XboxOutcome) {
    use windows_sys::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
    };
    use windows_sys::Win32::System::Registry::{
        HKEY, KEY_READ, REG_SZ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    };
    let subkey = wide(crate::gaming::XBOX_CONFIG_SUBKEY);
    let value = wide(crate::gaming::XBOX_CONFIG_VALUE);
    let mut key: HKEY = std::ptr::null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_READ,
            &mut key,
        )
    };
    if opened != ERROR_SUCCESS {
        let read = if opened == ERROR_FILE_NOT_FOUND || opened == ERROR_PATH_NOT_FOUND {
            XboxRead::Absent
        } else {
            XboxRead::Unreadable
        };
        return classify_xbox(&read);
    }
    let read = (|| {
        let mut units = vec![0u16; XBOX_VALUE_CAP_UNITS];
        let mut len_bytes = (XBOX_VALUE_CAP_UNITS as u32) * 2;
        let mut kind = 0;
        let queried = unsafe {
            RegQueryValueExW(
                key,
                value.as_ptr(),
                std::ptr::null(),
                &mut kind,
                units.as_mut_ptr().cast(),
                &mut len_bytes,
            )
        };
        if queried == ERROR_FILE_NOT_FOUND || queried == ERROR_PATH_NOT_FOUND {
            return XboxRead::Absent;
        }
        // Wrong type, denial, or over-cap content: present but unusable
        // (unknown either way, never a match).
        if queried == ERROR_MORE_DATA || queried != ERROR_SUCCESS || kind != REG_SZ {
            return XboxRead::Unreadable;
        }
        let cap_bytes = (XBOX_VALUE_CAP_UNITS as u32) * 2;
        // Exact reported byte length (odd lengths included) reaches the pure
        // classifier, which rejects them conservatively.
        let raw: Vec<u8> = unsafe {
            std::slice::from_raw_parts(units.as_ptr().cast(), len_bytes.min(cap_bytes) as usize)
        }
        .to_vec();
        classify_xbox_bytes(&raw)
    })();
    unsafe {
        RegCloseKey(key);
    }
    classify_xbox(&read)
}

/// Read-only Game Bar probe: count-only family query for the current user.
/// A counted match reports registered; a successful empty query reports
/// absent; any query failure reports unknown (never absent). No full names
/// are fetched, so no inventory exists to log.
#[must_use]
pub fn probe_gamebar() -> (crate::gaming::GameBarPackage, crate::gaming::GameBarOutcome) {
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;
    let family = wide(GAMEBAR_PACKAGE_FAMILY);
    let mut count: u32 = 0;
    let mut length: u32 = 0;
    let queried = unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
        )
    };
    if queried == ERROR_SUCCESS {
        return classify_package_query(true, count);
    }
    if queried == ERROR_INSUFFICIENT_BUFFER {
        // Null buffer with matches present: the count alone proves
        // registration (a zero count here is impossible, but stays unknown
        // rather than guessing).
        if count > 0 {
            return classify_package_query(true, count);
        }
    }
    classify_package_query(false, 0)
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
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    if !is_allowed_navigation_uri(uri) {
        return false;
    }
    let verb = wide("open");
    let target = wide(uri);
    let launched = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    (launched as usize) > 32
}
