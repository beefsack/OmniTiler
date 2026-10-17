//! Raw gaming-coexistence Win32 reads (no policy classification).
//!
//! Every registry/package/shell call lives here. Buffers are allocated
//! here and returned as owned values; classification (`classify_*`) and the
//! navigation allowlist stay with the safe policy callers, which import
//! only owned snapshots. No window, no state pointer, no callback.

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_PATH_NOT_FOUND};

const VALUE_CAP_UNITS: usize = 256;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Raw Xbox home-app read: absent/unreadable markers plus the exact
/// reported byte payload (odd lengths included) for the safe classifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawXbox {
    Absent,
    Unreadable,
    Bytes(Vec<u8>),
}

/// Read-only Xbox value fetch within the unit cap. Always closes the key.
/// Wrong type, denial, or over-cap content reports `Unreadable`.
pub fn read_xbox_value(subkey: &str, value: &str) -> RawXbox {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, KEY_READ, REG_SZ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    };
    let subkey_w = wide(subkey);
    let value_w = wide(value);
    let mut key: HKEY = std::ptr::null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            subkey_w.as_ptr(),
            0,
            KEY_READ,
            &mut key,
        )
    };
    if opened != ERROR_SUCCESS {
        if opened == ERROR_FILE_NOT_FOUND || opened == ERROR_PATH_NOT_FOUND {
            return RawXbox::Absent;
        }
        return RawXbox::Unreadable;
    }
    let read = (|| {
        let mut units = vec![0u16; VALUE_CAP_UNITS];
        let mut len_bytes = (VALUE_CAP_UNITS as u32) * 2;
        let mut kind = 0;
        let queried = unsafe {
            RegQueryValueExW(
                key,
                value_w.as_ptr(),
                std::ptr::null(),
                &mut kind,
                units.as_mut_ptr().cast(),
                &mut len_bytes,
            )
        };
        if queried == ERROR_FILE_NOT_FOUND || queried == ERROR_PATH_NOT_FOUND {
            return RawXbox::Absent;
        }
        if queried == ERROR_MORE_DATA || queried != ERROR_SUCCESS || kind != REG_SZ {
            return RawXbox::Unreadable;
        }
        let cap_bytes = (VALUE_CAP_UNITS as u32) * 2;
        let raw: Vec<u8> = unsafe {
            std::slice::from_raw_parts(units.as_ptr().cast(), len_bytes.min(cap_bytes) as usize)
        }
        .to_vec();
        RawXbox::Bytes(raw)
    })();
    unsafe {
        RegCloseKey(key);
    }
    read
}

/// Raw Game Bar family query: `(ok, count)`. A counted match proves
/// registration; the safe caller maps through `classify_package_query`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawPackageQuery {
    pub ok: bool,
    pub count: u32,
}

/// Count-only package-family query for the current user. No names fetched.
pub fn query_package_family(family: &str) -> RawPackageQuery {
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;
    let family_w = wide(family);
    let mut count: u32 = 0;
    let mut length: u32 = 0;
    let queried = unsafe {
        GetPackagesByPackageFamily(
            family_w.as_ptr(),
            &mut count,
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
        )
    };
    if queried == ERROR_SUCCESS {
        return RawPackageQuery { ok: true, count };
    }
    if queried == ERROR_INSUFFICIENT_BUFFER && count > 0 {
        return RawPackageQuery { ok: true, count };
    }
    RawPackageQuery {
        ok: false,
        count: 0,
    }
}

/// Shell launch without allowlist checks (the safe caller gates first).
/// Returns true when `ShellExecuteW` reports success (`> 32`).
pub fn launch_uri(uri: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
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
