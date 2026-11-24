//! Physical output identity: active display paths joined to their monitor
//! device instance and EDID registry value.
//!
//! Route (read-only, no display-mode mutation): `QueryDisplayConfig` with
//! `QDC_ONLY_ACTIVE_PATHS` gives the active source/target pairs; the source
//! packet carries the exact GDI device name the monitor enumeration uses
//! (`\\.\DISPLAY1`), and the target packet carries the monitor device path.
//! That path is reduced to a SetupAPI device instance, opened read-only, and
//! its device-key `EDID` value is handed to the pure parser in
//! [`crate::tiling_sys::output_identity`].
//!
//! Every field comes from the parsed EDID. QDC's own `edidManufactureId` /
//! `edidProductCodeId` are deliberately unused: mixing their raw byte order
//! with parsed EDID order would produce two spellings of one identity.
//!
//! `connector` is always `None`. No physical-connection normalization is
//! proved on this platform: display ids, adapter LUID/target ids,
//! `connectorInstance`, device-instance UID hashes, and `DISPLAY` ordinals
//! are connection observations or instance ordinals, not durable physical
//! connection names. An unknown connector makes the core hold ambiguous
//! reconnects instead of picking a candidate.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use tiler_core::world_report::OutputIdentity;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DICS_FLAG_GLOBAL, DIGCF_PRESENT, DIREG_DEV, GUID_DEVCLASS_MONITOR, SP_DEVINFO_DATA,
    SetupDiDestroyDeviceInfoList, SetupDiGetClassDevsW, SetupDiOpenDevRegKey,
    SetupDiOpenDeviceInfoW,
};
use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Registry::{
    HKEY, KEY_READ, REG_BINARY, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};

use crate::tiling_sys::output_identity::{device_instance_id, parse_edid};

/// Upper bound on active display paths accepted from the OS. A real desktop
/// is far below this; an oversized answer is refused rather than allocated.
const MAX_ACTIVE_PATHS: u32 = 64;
/// Upper bound on mode-info entries for the same query.
const MAX_MODE_INFOS: u32 = 1024;
/// Upper bound on one device-key EDID value. Real panels ship the 128-byte
/// base block plus several extension blocks (256/384 bytes are common), so
/// the bound covers a full extension set while refusing an implausible blob.
/// The parser reads the base block only.
const MAX_EDID_BYTES: usize = 4096;
/// Minimum readable EDID value: one complete base block.
const MIN_EDID_BYTES: usize = 128;
/// Device-key value holding the raw EDID bytes (`REG_BINARY`).
const EDID_VALUE_NAME: [u16; 5] = [0x0045, 0x0044, 0x0049, 0x0044, 0];
/// Monitor device-key subkey the display stack stores the EDID value under
/// on real hardware (`Device Parameters\EDID`).
const EDID_SUBKEY: [u16; 18] = [
    0x0044, 0x0065, 0x0076, 0x0069, 0x0063, 0x0065, 0x0020, 0x0050, 0x0061, 0x0072, 0x0061, 0x006D,
    0x0065, 0x0074, 0x0065, 0x0072, 0x0073, 0,
];

/// Read the physical identity of every active output, keyed by the exact GDI
/// device name the monitor enumeration reports.
///
/// `None` only when the active-path query itself fails; the caller then
/// treats every output as unknown for this tick. A per-monitor failure
/// (unavailable source name, missing target name, unparsable device path,
/// refused device-instance open, unreadable registry value, short or
/// malformed EDID) yields [`OutputIdentity::unknown`] for that device and
/// never fails the whole enumeration.
#[must_use]
pub fn monitor_identities() -> Option<HashMap<String, OutputIdentity>> {
    let paths = query_active_paths()?;
    let mut identities: HashMap<String, OutputIdentity> = HashMap::new();
    for path in &paths {
        let Some(device) = source_gdi_name(path) else {
            // Without the source name the row cannot be joined to a monitor
            // area, so it contributes nothing. Never invent a key.
            continue;
        };
        let identity = target_identity(path);
        match identities.entry(device) {
            Entry::Occupied(mut slot) => {
                // Two active targets behind one GDI source: the identity is
                // ambiguous, so the device is explicitly unknown.
                slot.insert(OutputIdentity::unknown());
            }
            Entry::Vacant(slot) => {
                slot.insert(identity);
            }
        }
    }
    Some(identities)
}

/// Active display paths, with one bounded retry when the display
/// configuration changed between the size query and the path query.
fn query_active_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    for _ in 0..2 {
        let mut path_count: u32 = 0;
        let mut mode_count: u32 = 0;
        // SAFETY: both out-pointers are valid for the call; the API only
        // writes the two counts.
        let sized = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if sized != ERROR_SUCCESS {
            return None;
        }
        if path_count == 0
            || mode_count == 0
            || path_count > MAX_ACTIVE_PATHS
            || mode_count > MAX_MODE_INFOS
        {
            return None;
        }
        // SAFETY: `DISPLAYCONFIG_PATH_INFO` and `DISPLAYCONFIG_MODE_INFO`
        // are plain C structs whose zero bit pattern is a valid empty
        // value (integer fields and integer-only unions); the API fills
        // them below before any field is read.
        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> =
            vec![unsafe { std::mem::zeroed() }; path_count as usize];
        // SAFETY: same plain-struct argument; mode infos are never read.
        let mut modes: Vec<_> = vec![unsafe { std::mem::zeroed() }; mode_count as usize];
        let mut used_paths = path_count;
        let mut used_modes = mode_count;
        // SAFETY: the buffers are sized by the API's own size query and are
        // written only up to the returned counts; the topology out-pointer
        // is omitted (null) as documented for `QueryDisplayConfig`.
        let queried = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut used_paths,
                paths.as_mut_ptr(),
                &mut used_modes,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if queried == ERROR_SUCCESS {
            if used_paths > path_count {
                return None;
            }
            paths.truncate(used_paths as usize);
            return Some(paths);
        }
        if queried != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        // The configuration grew between the two calls: re-size once.
    }
    None
}

/// Exact GDI device name (`\\.\DISPLAY1`) of one active path's source.
fn source_gdi_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    let mut packet = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        viewGdiDeviceName: [0; 32],
    };
    // SAFETY: the packet header names its own type and size; the API fills
    // the fixed-size name array in place.
    let status = unsafe { DisplayConfigGetDeviceInfo(&mut packet.header) };
    if status != ERROR_SUCCESS as i32 {
        return None;
    }
    wide_text(&packet.viewGdiDeviceName)
}

/// Identity of one active path's target monitor, or explicit unknown.
fn target_identity(path: &DISPLAYCONFIG_PATH_INFO) -> OutputIdentity {
    let mut packet = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
    packet.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
    packet.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
    packet.header.adapterId = path.targetInfo.adapterId;
    packet.header.id = path.targetInfo.id;
    // SAFETY: the packet header names its own type and size; the API fills
    // the fixed-size path/name arrays in place.
    let status = unsafe { DisplayConfigGetDeviceInfo(&mut packet.header) };
    if status != ERROR_SUCCESS as i32 {
        return OutputIdentity::unknown();
    }
    // Only the device path is consumed. The friendly name is not physical
    // identity, and the raw EDID ids are not mixed with parsed values.
    let Some(monitor_path) = wide_text(&packet.monitorDevicePath) else {
        return OutputIdentity::unknown();
    };
    let Some(instance) = device_instance_id(&monitor_path) else {
        return OutputIdentity::unknown();
    };
    let Some(edid) = read_edid(&instance) else {
        return OutputIdentity::unknown();
    };
    parse_edid(&edid)
}

/// Read the device-key `EDID` value of one monitor device instance.
fn read_edid(device_instance: &str) -> Option<Vec<u8>> {
    let instance: Vec<u16> = device_instance
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: the class GUID is the static monitor class; enumerator and
    // owner window are absent (null) by design; the returned set is owned
    // by the RAII guard below.
    let set = unsafe {
        SetupDiGetClassDevsW(
            &GUID_DEVCLASS_MONITOR,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT,
        )
    };
    if set == INVALID_HANDLE_VALUE as isize {
        return None;
    }
    let set = DeviceSet(set);
    // SAFETY: `SP_DEVINFO_DATA` is a plain C struct whose zero bit pattern
    // is valid; `cbSize` is set immediately.
    let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
    // SAFETY: the set is live, the NUL-terminated instance id is valid for
    // the call, and `info` is a valid out-pointer.
    let opened = unsafe {
        SetupDiOpenDeviceInfoW(set.0, instance.as_ptr(), std::ptr::null_mut(), 0, &mut info)
    };
    if opened == 0 {
        return None;
    }
    // SAFETY: the set and opened device info are live; the returned key is
    // owned by the RAII guard below and closed on every path.
    let key =
        unsafe { SetupDiOpenDevRegKey(set.0, &info, DICS_FLAG_GLOBAL, 0, DIREG_DEV, KEY_READ) };
    if key.is_null() || key == INVALID_HANDLE_VALUE {
        return None;
    }
    let key = DevKey(key);
    if let Some(edid) = read_edid_value(key.0) {
        return Some(edid);
    }
    // Real monitor stacks keep the raw value under the device key's
    // `Device Parameters` subkey; the hardware key itself does not always
    // carry it. Both locations are read-only and bounded.
    let mut subkey: HKEY = std::ptr::null_mut();
    // SAFETY: the parent key is live, the subkey name is NUL-terminated,
    // and `subkey` is a valid out-pointer owned by the RAII guard below.
    let opened = unsafe { RegOpenKeyExW(key.0, EDID_SUBKEY.as_ptr(), 0, KEY_READ, &mut subkey) };
    if opened != ERROR_SUCCESS || subkey.is_null() || subkey == INVALID_HANDLE_VALUE {
        return None;
    }
    let subkey = DevKey(subkey);
    read_edid_value(subkey.0)
}

/// Read the `EDID` value from one registry key, bounded and type-checked.
fn read_edid_value(key: HKEY) -> Option<Vec<u8>> {
    let mut value_type: u32 = 0;
    let mut length: u32 = 0;
    // SAFETY: the key is live; a null data pointer with a valid length
    // out-pointer asks the API for the value's size and type only.
    let sized = unsafe {
        RegQueryValueExW(
            key,
            EDID_VALUE_NAME.as_ptr(),
            std::ptr::null(),
            &mut value_type,
            std::ptr::null_mut(),
            &mut length,
        )
    };
    if sized != ERROR_SUCCESS {
        return None;
    }
    if value_type != REG_BINARY {
        return None;
    }
    let length = length as usize;
    if !(MIN_EDID_BYTES..=MAX_EDID_BYTES).contains(&length) {
        return None;
    }
    let mut bytes = vec![0u8; length];
    let mut read: u32 = length as u32;
    // SAFETY: the key is live and `bytes` is valid for `read` bytes, which
    // the API updates to the bytes actually written (never above the
    // supplied length).
    let loaded = unsafe {
        RegQueryValueExW(
            key,
            EDID_VALUE_NAME.as_ptr(),
            std::ptr::null(),
            &mut value_type,
            bytes.as_mut_ptr(),
            &mut read,
        )
    };
    if loaded != ERROR_SUCCESS {
        return None;
    }
    let read = (read as usize).min(bytes.len());
    if read < MIN_EDID_BYTES {
        return None;
    }
    bytes.truncate(read);
    Some(bytes)
}

/// NUL-terminated UTF-16 payload as text; `None` when empty.
fn wide_text(value: &[u16]) -> Option<String> {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    if end == 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&value[..end]);
    if text.is_empty() { None } else { Some(text) }
}

/// Owned SetupAPI device information set; destroyed exactly once.
struct DeviceSet(isize);

impl Drop for DeviceSet {
    fn drop(&mut self) {
        // SAFETY: the handle came from `SetupDiGetClassDevsW` and is closed
        // exactly once, here.
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

/// Owned device registry key; closed exactly once.
struct DevKey(HKEY);

impl Drop for DevKey {
    fn drop(&mut self) {
        // SAFETY: the key came from `SetupDiOpenDevRegKey` and is closed
        // exactly once, here.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::monitor_identities;

    /// Live, read-only host probe: one `QueryDisplayConfig` active-path
    /// enumeration joined to each target's SetupAPI device instance and
    /// EDID value, under the product process. Needs real displays and a
    /// readable monitor device key; mutates nothing (no mode, scale,
    /// window, or setting) and prints only device names plus per-field
    /// presence, never identity text. Ignored by default; run with
    /// `-- --ignored --nocapture`.
    #[test]
    #[ignore = "live QDC/SetupAPI read-only"]
    fn live_monitor_identities_are_readable() {
        let identities = monitor_identities().expect("active display paths");
        assert!(!identities.is_empty(), "at least one active output");
        for (device, identity) in &identities {
            println!(
                "{device}: manufacturer={} model={} serial={} connector={}",
                identity.manufacturer.is_some(),
                identity.model.is_some(),
                identity.serial.is_some(),
                identity.connector.is_some(),
            );
            assert!(
                device.starts_with(r"\\.\DISPLAY"),
                "GDI device name shape: {device}"
            );
        }
        assert!(
            identities.values().any(|identity| identity.manufacturer.is_some()
                || identity.model.is_some()
                || identity.serial.is_some()),
            "at least one identity field is known"
        );
    }
}
