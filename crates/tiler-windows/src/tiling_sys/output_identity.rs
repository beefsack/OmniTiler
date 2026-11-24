#![forbid(unsafe_code)]
//! Neutral output identity facts read from the native presentation seam.
//!
//! This module is pure: it turns raw EDID bytes into the core's
//! [`OutputIdentity`] and normalizes a monitor device path into a SetupAPI
//! device instance ID. Neither function invents a value: every unreadable or
//! malformed field is `None`, and the connector stays `None` because EDID
//! carries no connector and physical-connection normalization is not proved
//! on this platform.

use tiler_core::world_report::{MAX_IDENTITY_TEXT_BYTES, OutputIdentity};

/// Standard EDID base-block header (bytes 0..8).
const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// One EDID base block: extension blocks are never read.
const EDID_BASE_BLOCK_LEN: usize = 128;

/// Descriptor block starts (18 bytes each); block 2 also carries the
/// extension count at its final byte and block 3 the checksum.
const DESCRIPTOR_OFFSETS: [usize; 4] = [54, 72, 90, 108];
const DESCRIPTOR_LEN: usize = 18;
/// Descriptor tag byte: display product name.
const TEXT_TAG_MODEL: u8 = 0xFC;
/// Descriptor tag byte: display serial number.
const TEXT_TAG_SERIAL: u8 = 0xFF;
/// Text payload: 13 bytes, block bytes 5..18.
const TEXT_PAYLOAD_START: usize = 5;
const TEXT_PAYLOAD_LEN: usize = 13;
const TEXT_TERMINATOR_LF: u8 = 0x0A;
const TEXT_TERMINATOR_NUL: u8 = 0x00;

/// Upper bound on accepted monitor device paths.
const MAX_MONITOR_DEVICE_PATH_BYTES: usize = 512;

/// Decode the first 128-byte EDID base block into neutral identity facts.
///
/// Fewer than 128 bytes, a bad header, or a malformed descriptor yields
/// per-field `None`; the function never panics and never fabricates a value.
/// `connector` is always `None`: EDID has no connector, and this platform
/// does not prove physical-connection normalization.
#[must_use]
pub(crate) fn parse_edid(bytes: &[u8]) -> OutputIdentity {
    if bytes.len() < EDID_BASE_BLOCK_LEN {
        return OutputIdentity::unknown();
    }
    let base = &bytes[..EDID_BASE_BLOCK_LEN];
    if base[..EDID_HEADER.len()] != EDID_HEADER {
        return OutputIdentity::unknown();
    }

    let manufacturer = decode_manufacturer(base);
    let model = decode_text_descriptor(base, TEXT_TAG_MODEL).or_else(|| decode_product_code(base));
    let serial =
        decode_text_descriptor(base, TEXT_TAG_SERIAL).or_else(|| decode_numeric_serial(base));

    OutputIdentity {
        manufacturer,
        model,
        serial,
        connector: None,
    }
}

/// Decode the three 5-bit manufacturer letters (big-endian word at 8..10).
///
/// Reserved bit 15 or any code outside `1..=26` makes the whole field
/// unknown: a partial spelling is not an identity.
fn decode_manufacturer(base: &[u8]) -> Option<String> {
    let word = u16::from_be_bytes([base[8], base[9]]);
    if word & 0x8000 != 0 {
        return None;
    }
    let mut letters = String::with_capacity(3);
    for shift in [10, 5, 0] {
        let code = (word >> shift) & 0x1F;
        if !(1..=26).contains(&code) {
            return None;
        }
        letters.push(char::from(b'A' + u8::try_from(code).ok()? - 1));
    }
    Some(letters)
}

/// First valid text descriptor with `tag`, or `None` when every candidate is
/// invalid. An invalid candidate is skipped, never repaired.
fn decode_text_descriptor(base: &[u8], tag: u8) -> Option<String> {
    for offset in DESCRIPTOR_OFFSETS {
        let block = &base[offset..offset + DESCRIPTOR_LEN];
        if block[0] != 0 || block[1] != 0 || block[2] != 0 || block[3] != tag {
            continue;
        }
        let payload = &block[TEXT_PAYLOAD_START..TEXT_PAYLOAD_START + TEXT_PAYLOAD_LEN];
        if let Some(text) = decode_text_payload(payload) {
            return Some(text);
        }
    }
    None
}

/// Decode one 13-byte text payload: stop at the first LF or NUL, require a
/// clean terminator fill, printable ASCII text, and a non-blank trimmed
/// result. Anything else is an invalid candidate.
fn decode_text_payload(payload: &[u8]) -> Option<String> {
    let mut end = payload.len();
    let mut terminator = None;
    for (index, &byte) in payload.iter().enumerate() {
        if byte == TEXT_TERMINATOR_LF || byte == TEXT_TERMINATOR_NUL {
            end = index;
            terminator = Some(byte);
            break;
        }
    }
    let rest = &payload[end..];
    let fill_start = usize::from(terminator.is_some());
    let fill = &rest[fill_start..];
    let fill_is_clean = match terminator {
        Some(TEXT_TERMINATOR_LF) => fill
            .iter()
            .all(|&byte| byte == TEXT_TERMINATOR_NUL || byte == b' '),
        Some(_) => fill.iter().all(|&byte| byte == TEXT_TERMINATOR_NUL),
        None => true,
    };
    if !fill_is_clean {
        return None;
    }

    let text = &payload[..end];
    if text.iter().any(|&byte| !(0x20..=0x7E).contains(&byte)) {
        return None;
    }
    let text = core::str::from_utf8(text).ok()?.trim();
    if text.is_empty() || text.len() > MAX_IDENTITY_TEXT_BYTES {
        return None;
    }
    Some(text.to_owned())
}

/// Product-code fallback: little-endian u16 at 10..12 as four uppercase hex
/// digits; `0` and `0xFFFF` are not identities.
fn decode_product_code(base: &[u8]) -> Option<String> {
    let product = u16::from_le_bytes([base[10], base[11]]);
    if product == 0 || product == u16::MAX {
        return None;
    }
    Some(format!("{product:04X}"))
}

/// Numeric-serial fallback: little-endian u32 at 12..16 in decimal; `0` and
/// `0xFFFF_FFFF` are not identities.
fn decode_numeric_serial(base: &[u8]) -> Option<String> {
    let serial = u32::from_le_bytes([base[12], base[13], base[14], base[15]]);
    if serial == 0 || serial == u32::MAX {
        return None;
    }
    Some(serial.to_string())
}

/// Turn a `monitorDevicePath` into a SetupAPI device instance ID.
///
/// Accepts the two native shapes only: an enumerated display interface path
/// (`\\?\DISPLAY#<hardware>#<instance>[#{guid}]`, normalized to
/// `DISPLAY\<hardware>\<instance>`) and an already-normalized instance ID
/// (`DISPLAY\<hardware>\<instance>`). Anything else is `None`; no prefix is
/// invented and non-DISPLAY buses are rejected.
#[must_use]
pub(crate) fn device_instance_id(monitor_device_path: &str) -> Option<String> {
    if monitor_device_path.len() > MAX_MONITOR_DEVICE_PATH_BYTES {
        return None;
    }
    let path = monitor_device_path
        .strip_prefix(r"\\?\")
        .unwrap_or(monitor_device_path);

    if path.contains('#') {
        let segments: Vec<&str> = path.split('#').collect();
        if !(segments.len() == 3 || segments.len() == 4) {
            return None;
        }
        if segments.iter().any(|segment| segment.is_empty()) {
            return None;
        }
        if !segments[0].eq_ignore_ascii_case("DISPLAY") {
            return None;
        }
        if segments.len() == 4 {
            let interface = segments[3];
            if !(interface.starts_with('{') && interface.ends_with('}')) {
                return None;
            }
        }
        return Some(format!("DISPLAY\\{}\\{}", segments[1], segments[2]));
    }

    if path.contains('\\') {
        let segments: Vec<&str> = path.split('\\').collect();
        if segments.len() != 3 {
            return None;
        }
        if segments.iter().any(|segment| segment.is_empty()) {
            return None;
        }
        if !segments[0].eq_ignore_ascii_case("DISPLAY") {
            return None;
        }
        return Some(path.to_owned());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 128-byte base block with a valid header and manufacturer `DEL`
    /// (D=4, E=5, L=12), product code `0x1234`, numeric serial `0`.
    fn base_edid() -> Vec<u8> {
        let mut bytes = vec![0u8; EDID_BASE_BLOCK_LEN];
        bytes[..EDID_HEADER.len()].copy_from_slice(&EDID_HEADER);
        let manufacturer_word: u16 = (4 << 10) | (5 << 5) | 12;
        bytes[8..10].copy_from_slice(&manufacturer_word.to_be_bytes());
        bytes[10..12].copy_from_slice(&0x1234_u16.to_le_bytes());
        bytes[12..16].copy_from_slice(&0_u32.to_le_bytes());
        bytes
    }

    /// 18-byte text descriptor with `tag` and the given 13-byte payload.
    fn text_descriptor(tag: u8, payload: [u8; TEXT_PAYLOAD_LEN]) -> [u8; DESCRIPTOR_LEN] {
        let mut block = [0u8; DESCRIPTOR_LEN];
        block[3] = tag;
        block[TEXT_PAYLOAD_START..TEXT_PAYLOAD_START + TEXT_PAYLOAD_LEN].copy_from_slice(&payload);
        block
    }

    fn set_descriptor(bytes: &mut [u8], slot: usize, block: [u8; DESCRIPTOR_LEN]) {
        let offset = DESCRIPTOR_OFFSETS[slot];
        bytes[offset..offset + DESCRIPTOR_LEN].copy_from_slice(&block);
    }

    /// NUL-padded payload for an ASCII text.
    fn payload_of(text: &[u8]) -> [u8; TEXT_PAYLOAD_LEN] {
        assert!(text.len() <= TEXT_PAYLOAD_LEN);
        let mut payload = [0u8; TEXT_PAYLOAD_LEN];
        payload[..text.len()].copy_from_slice(text);
        payload
    }

    fn set_product(bytes: &mut [u8], product: u16) {
        bytes[10..12].copy_from_slice(&product.to_le_bytes());
    }

    fn set_numeric_serial(bytes: &mut [u8], serial: u32) {
        bytes[12..16].copy_from_slice(&serial.to_le_bytes());
    }

    #[test]
    fn short_or_empty_input_is_unknown() {
        assert_eq!(parse_edid(&[]), OutputIdentity::unknown());
        assert_eq!(
            parse_edid(&[0u8; EDID_BASE_BLOCK_LEN - 1]),
            OutputIdentity::unknown()
        );
        let mut truncated = base_edid();
        truncated.truncate(EDID_BASE_BLOCK_LEN - 1);
        assert_eq!(parse_edid(&truncated), OutputIdentity::unknown());
    }

    #[test]
    fn wrong_header_is_unknown() {
        let mut bytes = base_edid();
        bytes[0] = 0x01;
        assert_eq!(parse_edid(&bytes), OutputIdentity::unknown());

        let mut bytes = base_edid();
        bytes[7] = 0x01;
        assert_eq!(parse_edid(&bytes), OutputIdentity::unknown());
    }

    #[test]
    fn manufacturer_decodes_three_letters() {
        let identity = parse_edid(&base_edid());
        assert_eq!(identity.manufacturer.as_deref(), Some("DEL"));
        assert_eq!(identity.model.as_deref(), Some("1234"));
        assert_eq!(identity.serial, None);
    }

    #[test]
    fn manufacturer_with_zero_code_is_unknown() {
        let mut bytes = base_edid();
        let word: u16 = (5 << 5) | 12;
        bytes[8..10].copy_from_slice(&word.to_be_bytes());
        let identity = parse_edid(&bytes);
        assert_eq!(identity.manufacturer, None);
        // The rest of the identity is still read independently.
        assert_eq!(identity.model.as_deref(), Some("1234"));
    }

    #[test]
    fn manufacturer_with_reserved_bit_is_unknown() {
        let mut bytes = base_edid();
        let word: u16 = 0x8000 | (4 << 10) | (5 << 5) | 12;
        bytes[8..10].copy_from_slice(&word.to_be_bytes());
        assert_eq!(parse_edid(&bytes).manufacturer, None);
    }

    #[test]
    fn product_code_fallback_is_uppercase_hex() {
        let mut bytes = base_edid();
        set_product(&mut bytes, 0x1A2B);
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("1A2B"));

        let mut bytes = base_edid();
        set_product(&mut bytes, 0x00AB);
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("00AB"));
    }

    #[test]
    fn unusable_product_code_is_none() {
        let mut bytes = base_edid();
        set_product(&mut bytes, 0);
        assert_eq!(parse_edid(&bytes).model, None);

        let mut bytes = base_edid();
        set_product(&mut bytes, 0xFFFF);
        assert_eq!(parse_edid(&bytes).model, None);
    }

    #[test]
    fn model_text_descriptor_lf_terminated() {
        let mut bytes = base_edid();
        let mut payload = payload_of(b"X2719");
        payload[5] = TEXT_TERMINATOR_LF;
        payload[6] = b' ';
        set_descriptor(&mut bytes, 0, text_descriptor(TEXT_TAG_MODEL, payload));
        let identity = parse_edid(&bytes);
        assert_eq!(identity.model.as_deref(), Some("X2719"));
    }

    #[test]
    fn model_text_trailing_space_padding_is_trimmed() {
        let mut bytes = base_edid();
        // No terminator: all 13 payload bytes are text, trailing spaces padded.
        let mut payload = [b' '; TEXT_PAYLOAD_LEN];
        payload[..4].copy_from_slice(b"U241");
        set_descriptor(&mut bytes, 0, text_descriptor(TEXT_TAG_MODEL, payload));
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("U241"));
    }

    #[test]
    fn model_text_interior_nul_falls_back_to_product_code() {
        let mut bytes = base_edid();
        set_product(&mut bytes, 0x1A2B);
        let mut payload = [0u8; TEXT_PAYLOAD_LEN];
        payload[..2].copy_from_slice(b"AB");
        payload[3..5].copy_from_slice(b"CD");
        set_descriptor(&mut bytes, 0, text_descriptor(TEXT_TAG_MODEL, payload));
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("1A2B"));
    }

    #[test]
    fn model_text_non_printable_falls_back_to_product_code() {
        let mut bytes = base_edid();
        set_product(&mut bytes, 0x1A2B);
        let mut payload = payload_of(b"AB");
        payload[2] = 0x07;
        set_descriptor(&mut bytes, 0, text_descriptor(TEXT_TAG_MODEL, payload));
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("1A2B"));
    }

    #[test]
    fn blank_model_text_falls_back_to_product_code() {
        let mut bytes = base_edid();
        set_product(&mut bytes, 0x1A2B);
        set_descriptor(
            &mut bytes,
            0,
            text_descriptor(TEXT_TAG_MODEL, [b' '; TEXT_PAYLOAD_LEN]),
        );
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("1A2B"));
    }

    #[test]
    fn serial_text_is_preferred_over_numeric_serial() {
        let mut bytes = base_edid();
        set_numeric_serial(&mut bytes, 4_352);
        let payload = payload_of(b"SN-0042");
        set_descriptor(&mut bytes, 1, text_descriptor(TEXT_TAG_SERIAL, payload));
        assert_eq!(parse_edid(&bytes).serial.as_deref(), Some("SN-0042"));
    }

    #[test]
    fn numeric_serial_falls_back_to_decimal() {
        let mut bytes = base_edid();
        set_numeric_serial(&mut bytes, 4_352);
        assert_eq!(parse_edid(&bytes).serial.as_deref(), Some("4352"));
    }

    #[test]
    fn unusable_numeric_serial_is_none() {
        let mut bytes = base_edid();
        set_numeric_serial(&mut bytes, 0);
        assert_eq!(parse_edid(&bytes).serial, None);

        let mut bytes = base_edid();
        set_numeric_serial(&mut bytes, u32::MAX);
        assert_eq!(parse_edid(&bytes).serial, None);
    }

    #[test]
    fn blank_serial_text_falls_back_to_numeric_serial() {
        let mut bytes = base_edid();
        set_numeric_serial(&mut bytes, 7_001);
        set_descriptor(
            &mut bytes,
            2,
            text_descriptor(TEXT_TAG_SERIAL, [b' '; TEXT_PAYLOAD_LEN]),
        );
        assert_eq!(parse_edid(&bytes).serial.as_deref(), Some("7001"));
    }

    #[test]
    fn blank_serial_text_with_unusable_numeric_serial_is_none() {
        let mut bytes = base_edid();
        set_numeric_serial(&mut bytes, 0);
        set_descriptor(
            &mut bytes,
            2,
            text_descriptor(TEXT_TAG_SERIAL, [b' '; TEXT_PAYLOAD_LEN]),
        );
        assert_eq!(parse_edid(&bytes).serial, None);
    }

    #[test]
    fn first_valid_descriptor_with_same_tag_wins() {
        let mut bytes = base_edid();
        set_descriptor(
            &mut bytes,
            0,
            text_descriptor(TEXT_TAG_MODEL, payload_of(b"FIRST")),
        );
        set_descriptor(
            &mut bytes,
            1,
            text_descriptor(TEXT_TAG_MODEL, payload_of(b"SECOND")),
        );
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("FIRST"));
    }

    #[test]
    fn invalid_descriptor_is_skipped_for_a_later_valid_one() {
        let mut bytes = base_edid();
        set_product(&mut bytes, 0x1A2B);
        let mut interior_nul = [0u8; TEXT_PAYLOAD_LEN];
        interior_nul[0] = b'A';
        interior_nul[2] = b'B';
        set_descriptor(&mut bytes, 0, text_descriptor(TEXT_TAG_MODEL, interior_nul));
        set_descriptor(
            &mut bytes,
            1,
            text_descriptor(TEXT_TAG_MODEL, payload_of(b"LATER")),
        );
        assert_eq!(parse_edid(&bytes).model.as_deref(), Some("LATER"));
    }

    #[test]
    fn extension_count_byte_does_not_gate_parsing() {
        let mut bytes = base_edid();
        bytes[126] = 2;
        let identity = parse_edid(&bytes);
        assert_eq!(identity.manufacturer.as_deref(), Some("DEL"));
        assert_eq!(identity.model.as_deref(), Some("1234"));
    }

    #[test]
    fn connector_is_always_none() {
        let cases = [
            parse_edid(&base_edid()),
            parse_edid(&[]),
            parse_edid(&[0u8; EDID_BASE_BLOCK_LEN]),
        ];
        for identity in cases {
            assert_eq!(identity.connector, None);
        }

        let mut bytes = base_edid();
        set_numeric_serial(&mut bytes, 9);
        set_descriptor(
            &mut bytes,
            2,
            text_descriptor(TEXT_TAG_SERIAL, payload_of(b"SER")),
        );
        assert_eq!(parse_edid(&bytes).connector, None);
    }

    #[test]
    fn enumerated_display_interface_path_is_normalized() {
        let path = r"\\?\DISPLAY#DEL4098#5&2b1e&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert_eq!(
            device_instance_id(path).as_deref(),
            Some(r"DISPLAY\DEL4098\5&2b1e&0&UID4352")
        );
    }

    #[test]
    fn enumerated_display_interface_path_without_guid_is_normalized() {
        let path = r"\\?\DISPLAY#DEL4098#5&2b1e&0&UID4352";
        assert_eq!(
            device_instance_id(path).as_deref(),
            Some(r"DISPLAY\DEL4098\5&2b1e&0&UID4352")
        );
    }

    #[test]
    fn already_normalized_instance_id_is_unchanged() {
        let path = r"DISPLAY\DEL4098\5&2b1e&0&UID4352";
        assert_eq!(device_instance_id(path).as_deref(), Some(path));
    }

    #[test]
    fn non_display_bus_is_rejected() {
        let path =
            r"\\?\USB#VID_1D6B&PID_0002#5&2b1e&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert_eq!(device_instance_id(path), None);
        assert_eq!(device_instance_id(r"USB\VID_1D6B\5&2b1e"), None);
    }

    #[test]
    fn two_segment_hash_path_is_rejected() {
        assert_eq!(device_instance_id(r"\\?\DISPLAY#DEL4098"), None);
        assert_eq!(device_instance_id("DISPLAY#DEL4098"), None);
    }

    #[test]
    fn empty_instance_segment_is_rejected() {
        let path = r"\\?\DISPLAY#DEL4098##{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert_eq!(device_instance_id(path), None);
        assert_eq!(device_instance_id(r"DISPLAY\DEL4098\"), None);
    }

    #[test]
    fn single_token_path_is_rejected() {
        assert_eq!(device_instance_id("display-device-1"), None);
        assert_eq!(device_instance_id(""), None);
        assert_eq!(device_instance_id(r"\\?\DISPLAY"), None);
    }
}
