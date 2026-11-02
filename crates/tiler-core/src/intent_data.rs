//! Core-owned opaque persisted intent data codec.
//!
//! The adapter stores the inner `data` string unchanged in a lifetime-bound
//! native slot (or an adapter-owned atomic external slot) and returns it
//! unchanged; it never interprets the bytes as policy. Only the core decodes
//! and validates them here and decides save/restore. An invalid inner version
//! or shape yields no usable intent and no write authority from that record:
//! the caller ignores the inner data inside an otherwise accepted envelope,
//! never turning it into facts or write authority. That inner-data verdict is
//! narrow: outer type/byte-bound validation (the record `data` must be a
//! string within the slot bound) remains an independent whole-envelope
//! responsibility and is NOT bypassed by ignoring invalid inner data.
//!
//! Inner record contract (independent of the outer wire version): exactly
//! `{"v":1,"float":bool|null,"stickyPrior":bool|null}` in table order `v`,
//! `float`, `stickyPrior`, at most [`MAX_INTENT_DATA_BYTES`] UTF-8 bytes.
//! `float` `None` means no explicit float/tile override; `sticky_prior`
//! `None` means not sticky. No identity, adapter id, executable, geometry,
//! home, ring, history, fullscreen, or maximized state is persisted here, and
//! decoding never manufactures a choice: explicit `null` decodes to `None`,
//! while an absent required key refuses with `MissingField`.
//!
//! Decoding is strict: JSON whitespace, key order, and string escapes are
//! accepted (escaped keys compare by decoded value), while unknown keys,
//! duplicate decoded keys (including escaped equivalents), missing keys,
//! wrong-typed values, non-`1` versions, malformed JSON, trailing bytes, and
//! UTF-8-byte overbounds all refuse with a fixed [`IntentDataError`] that
//! never echoes payload bytes.

/// Opaque intent data bound: at most 1024 UTF-8 bytes (byte length, not
/// character count). Shared with the adapter storage slot obligation.
pub const MAX_INTENT_DATA_BYTES: usize = 1024;

/// Persisted intent data version, independent of the outer wire version.
/// Only this version decodes; anything else is [`IntentDataError::UnsupportedVersion`].
pub const INTENT_DATA_VERSION: u32 = 1;

/// Core-owned persisted intent: the explicit float/tile override plus the
/// retained sticky intent and prior float/tile intent for sticky-off.
/// `None` on either lane means "no override" / "not sticky"; it never
/// manufactures a user choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentData {
    /// Explicit float (`Some(true)`) / tile (`Some(false)`) override;
    /// `None` means no override.
    pub float: Option<bool>,
    /// Retained sticky intent and prior float/tile intent for sticky-off;
    /// `None` means not sticky.
    pub sticky_prior: Option<bool>,
}

impl IntentData {
    /// Typed construction of one of the nine semantic states.
    #[must_use]
    pub const fn new(float: Option<bool>, sticky_prior: Option<bool>) -> Self {
        Self {
            float,
            sticky_prior,
        }
    }
}

/// Fixed typed refusal for an unusable inner record. Kinds and messages are
/// fixed redacted vocabularies; they never echo payload bytes. Every variant
/// means the inner record is unusable (no intent, no write authority from
/// it); outer envelope validation stays an independent responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentDataError {
    /// Inner string exceeds [`MAX_INTENT_DATA_BYTES`] UTF-8 bytes.
    Overbound,
    /// Malformed JSON, a non-object top level, or trailing bytes.
    Malformed,
    /// A required key (`v`, `float`, `stickyPrior`) is absent.
    MissingField,
    /// A key outside `v`/`float`/`stickyPrior` is present.
    UnknownField,
    /// A decoded key repeats, including escaped equivalents (no last-key-wins).
    DuplicateField,
    /// A value has the wrong JSON type (no coercion: `1` is not `true`,
    /// `"1"` is not `1`).
    UnexpectedType,
    /// A well-formed JSON number version other than `1`.
    UnsupportedVersion,
}

impl IntentDataError {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Overbound => "overbound",
            Self::Malformed => "malformed",
            Self::MissingField => "missing-field",
            Self::UnknownField => "unknown-field",
            Self::DuplicateField => "duplicate-field",
            Self::UnexpectedType => "unexpected-type",
            Self::UnsupportedVersion => "unsupported-version",
        }
    }

    /// Fixed redacted message; never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Overbound => "intent data exceeds size bound",
            Self::Malformed => "intent data is malformed",
            Self::MissingField => "intent data is missing a required field",
            Self::UnknownField => "intent data contains an unknown field",
            Self::DuplicateField => "intent data contains a duplicate field",
            Self::UnexpectedType => "intent data contains an unexpected value type",
            Self::UnsupportedVersion => "unsupported intent data version",
        }
    }
}

/// Canonical encoder: exactly
/// `{"v":1,"float":<bool|null>,"stickyPrior":<bool|null>}` in table order.
#[must_use]
pub fn encode_intent_data(intent: &IntentData) -> String {
    fn word(value: Option<bool>) -> &'static str {
        match value {
            Some(true) => "true",
            Some(false) => "false",
            None => "null",
        }
    }
    let mut out = String::with_capacity(48);
    out.push_str("{\"v\":1,\"float\":");
    out.push_str(word(intent.float));
    out.push_str(",\"stickyPrior\":");
    out.push_str(word(intent.sticky_prior));
    out.push('}');
    out
}

/// Strict decoder for the inner record. Accepts JSON whitespace, key order,
/// and string escapes; refuses unknown/duplicate/missing keys, wrong-typed
/// values, non-`1` versions, malformed or trailing JSON, and UTF-8-byte
/// overbounds. Any [`IntentDataError`] means the inner record is unusable;
/// the caller keeps the otherwise accepted envelope and drops only the inner
/// intent. The decoder's own `<=1024` byte check is retained exactly, but it
/// does not replace outer record validation: an overbound or wrong-typed
/// outer `data` field remains a whole-envelope rejection owned elsewhere.
pub fn decode_intent_data(data: &str) -> Result<IntentData, IntentDataError> {
    if data.len() > MAX_INTENT_DATA_BYTES {
        return Err(IntentDataError::Overbound);
    }
    let mut cursor = Cursor {
        bytes: data.as_bytes(),
        text: data,
        pos: 0,
    };
    cursor.skip_ws();
    cursor.expect(b'{')?;
    cursor.skip_ws();
    // Slot order follows the table: 0 = v, 1 = float, 2 = stickyPrior.
    let mut seen = [false; 3];
    let mut float: Option<Option<bool>> = None;
    let mut sticky_prior: Option<Option<bool>> = None;
    if cursor.peek() == Some(b'}') {
        cursor.pos += 1;
    } else {
        loop {
            cursor.skip_ws();
            let slot = match cursor.parse_key()?.as_str() {
                "v" => 0,
                "float" => 1,
                "stickyPrior" => 2,
                _ => return Err(IntentDataError::UnknownField),
            };
            if seen[slot] {
                return Err(IntentDataError::DuplicateField);
            }
            seen[slot] = true;
            cursor.skip_ws();
            cursor.expect(b':')?;
            cursor.skip_ws();
            match slot {
                0 => cursor.parse_version()?,
                1 => float = Some(cursor.parse_nullable_bool()?),
                _ => sticky_prior = Some(cursor.parse_nullable_bool()?),
            }
            cursor.skip_ws();
            match cursor.peek() {
                Some(b',') => cursor.pos += 1,
                Some(b'}') => {
                    cursor.pos += 1;
                    break;
                }
                _ => return Err(IntentDataError::Malformed),
            }
        }
    }
    if !seen[0] || float.is_none() || sticky_prior.is_none() {
        return Err(IntentDataError::MissingField);
    }
    cursor.skip_ws();
    if cursor.pos != cursor.bytes.len() {
        return Err(IntentDataError::Malformed);
    }
    Ok(IntentData {
        float: float.unwrap_or(None),
        sticky_prior: sticky_prior.unwrap_or(None),
    })
}

struct Cursor<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl Cursor<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r')
        {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), IntentDataError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(IntentDataError::Malformed)
        }
    }

    fn parse_key(&mut self) -> Result<String, IntentDataError> {
        if self.peek() != Some(b'"') {
            return Err(IntentDataError::Malformed);
        }
        self.parse_json_string()
    }

    /// JSON string body with full escape handling (`\" \\ \/ \b \f \n \r
    /// \t`, `\uXXXX` with surrogate-pair combining). Unescaped controls,
    /// invalid escapes, and lone surrogates refuse as malformed. Byte
    /// scanning for `"`/`\` is UTF-8 safe (continuation bytes are >= 0x80).
    fn parse_json_string(&mut self) -> Result<String, IntentDataError> {
        debug_assert_eq!(self.peek(), Some(b'"'));
        self.pos += 1;
        let mut out = String::new();
        loop {
            let byte = self.peek().ok_or(IntentDataError::Malformed)?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    match self.peek().ok_or(IntentDataError::Malformed)? {
                        b'"' => {
                            out.push('"');
                            self.pos += 1;
                        }
                        b'\\' => {
                            out.push('\\');
                            self.pos += 1;
                        }
                        b'/' => {
                            out.push('/');
                            self.pos += 1;
                        }
                        b'b' => {
                            out.push('\u{0008}');
                            self.pos += 1;
                        }
                        b'f' => {
                            out.push('\u{000C}');
                            self.pos += 1;
                        }
                        b'n' => {
                            out.push('\n');
                            self.pos += 1;
                        }
                        b'r' => {
                            out.push('\r');
                            self.pos += 1;
                        }
                        b't' => {
                            out.push('\t');
                            self.pos += 1;
                        }
                        b'u' => {
                            self.pos += 1;
                            let unit = self.parse_hex4()?;
                            if (0xD800..0xDC00).contains(&unit) {
                                if self.peek() != Some(b'\\') {
                                    return Err(IntentDataError::Malformed);
                                }
                                self.pos += 1;
                                if self.peek() != Some(b'u') {
                                    return Err(IntentDataError::Malformed);
                                }
                                self.pos += 1;
                                let low = self.parse_hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(IntentDataError::Malformed);
                                }
                                let point = 0x1_0000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                                out.push(char::from_u32(point).ok_or(IntentDataError::Malformed)?);
                            } else if (0xDC00..0xE000).contains(&unit) {
                                return Err(IntentDataError::Malformed);
                            } else {
                                out.push(char::from_u32(unit).ok_or(IntentDataError::Malformed)?);
                            }
                        }
                        _ => return Err(IntentDataError::Malformed),
                    }
                }
                0x00..=0x1F => return Err(IntentDataError::Malformed),
                _ => {
                    let rest = self
                        .text
                        .get(self.pos..)
                        .ok_or(IntentDataError::Malformed)?;
                    let ch = rest.chars().next().ok_or(IntentDataError::Malformed)?;
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u32, IntentDataError> {
        if self.pos + 4 > self.bytes.len() {
            return Err(IntentDataError::Malformed);
        }
        let mut value: u32 = 0;
        for i in 0..4 {
            let digit = match self.bytes[self.pos + i] {
                b'0'..=b'9' => u32::from(self.bytes[self.pos + i] - b'0'),
                b'a'..=b'f' => u32::from(self.bytes[self.pos + i] - b'a') + 10,
                b'A'..=b'F' => u32::from(self.bytes[self.pos + i] - b'A') + 10,
                _ => return Err(IntentDataError::Malformed),
            };
            value = value * 16 + digit;
        }
        self.pos += 4;
        Ok(value)
    }

    fn parse_literal(&mut self, word: &str) -> Result<(), IntentDataError> {
        if self.bytes.len() >= self.pos + word.len()
            && &self.bytes[self.pos..self.pos + word.len()] == word.as_bytes()
        {
            self.pos += word.len();
            Ok(())
        } else {
            Err(IntentDataError::Malformed)
        }
    }

    /// Version lane: the integer literal `1` only. A well-formed JSON number
    /// other than `1` (including non-integer spellings like `1.0`/`1e0`: no
    /// coercion to version 1) is an unsupported version, and any non-number
    /// (string, bool, null, object, array) is a wrong type. The scanned
    /// number must terminate cleanly (whitespace, `,`, `}`, or input end)
    /// before any version verdict: `2x` is malformed, not a version.
    fn parse_version(&mut self) -> Result<(), IntentDataError> {
        let rest = &self.bytes[self.pos..];
        let first = rest.first().copied();
        if !first.is_some_and(|b| b == b'-' || b.is_ascii_digit()) {
            return Err(IntentDataError::UnexpectedType);
        }
        let mut i = 0;
        let at = |j: usize| rest.get(j).copied();
        if at(i) == Some(b'-') {
            i += 1;
        }
        if at(i) == Some(b'0') {
            i += 1;
            // JSON forbids leading zeros: `01` is malformed, not a version.
            if at(i).is_some_and(|b| b.is_ascii_digit()) {
                return Err(IntentDataError::Malformed);
            }
        } else if at(i).is_some_and(|b| b.is_ascii_digit()) {
            while at(i).is_some_and(|b| b.is_ascii_digit()) {
                i += 1;
            }
        } else {
            return Err(IntentDataError::Malformed);
        }
        if at(i) == Some(b'.') {
            i += 1;
            if !at(i).is_some_and(|b| b.is_ascii_digit()) {
                return Err(IntentDataError::Malformed);
            }
            while at(i).is_some_and(|b| b.is_ascii_digit()) {
                i += 1;
            }
        }
        if at(i) == Some(b'e') || at(i) == Some(b'E') {
            i += 1;
            if at(i) == Some(b'+') || at(i) == Some(b'-') {
                i += 1;
            }
            if !at(i).is_some_and(|b| b.is_ascii_digit()) {
                return Err(IntentDataError::Malformed);
            }
            while at(i).is_some_and(|b| b.is_ascii_digit()) {
                i += 1;
            }
        }
        let token = &rest[..i];
        self.pos += i;
        match self.peek() {
            None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b',')
            | Some(b'}') => {}
            _ => return Err(IntentDataError::Malformed),
        }
        if token == b"1" {
            Ok(())
        } else {
            Err(IntentDataError::UnsupportedVersion)
        }
    }

    /// Nullable-bool lane: exactly `true`, `false`, or `null`. Any other
    /// JSON value shape refuses without coercion.
    fn parse_nullable_bool(&mut self) -> Result<Option<bool>, IntentDataError> {
        match self.peek() {
            Some(b't') => {
                self.parse_literal("true")?;
                Ok(Some(true))
            }
            Some(b'f') => {
                self.parse_literal("false")?;
                Ok(Some(false))
            }
            Some(b'n') => {
                self.parse_literal("null")?;
                Ok(None)
            }
            Some(_) => Err(IntentDataError::UnexpectedType),
            None => Err(IntentDataError::Malformed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_ok(data: &str) -> IntentData {
        decode_intent_data(data).expect("valid intent data")
    }

    #[test]
    fn all_nine_semantic_states_roundtrip_canonically() {
        // (float, sticky_prior, canonical bytes): null vs false vs true on
        // each lane stay distinct through decode and canonical encode.
        let cases: [(Option<bool>, Option<bool>, &str); 9] = [
            (None, None, "{\"v\":1,\"float\":null,\"stickyPrior\":null}"),
            (
                None,
                Some(false),
                "{\"v\":1,\"float\":null,\"stickyPrior\":false}",
            ),
            (
                None,
                Some(true),
                "{\"v\":1,\"float\":null,\"stickyPrior\":true}",
            ),
            (
                Some(false),
                None,
                "{\"v\":1,\"float\":false,\"stickyPrior\":null}",
            ),
            (
                Some(false),
                Some(false),
                "{\"v\":1,\"float\":false,\"stickyPrior\":false}",
            ),
            (
                Some(false),
                Some(true),
                "{\"v\":1,\"float\":false,\"stickyPrior\":true}",
            ),
            (
                Some(true),
                None,
                "{\"v\":1,\"float\":true,\"stickyPrior\":null}",
            ),
            (
                Some(true),
                Some(false),
                "{\"v\":1,\"float\":true,\"stickyPrior\":false}",
            ),
            (
                Some(true),
                Some(true),
                "{\"v\":1,\"float\":true,\"stickyPrior\":true}",
            ),
        ];
        for (float, sticky_prior, canonical) in cases {
            let intent = IntentData::new(float, sticky_prior);
            assert_eq!(encode_intent_data(&intent), canonical);
            assert_eq!(decode_ok(canonical), intent);
        }
    }

    #[test]
    fn accepts_whitespace_key_order_and_escapes() {
        assert_eq!(
            decode_ok("{\n\t \"stickyPrior\" : null ,\r \"float\" : true , \"v\" : 1 }"),
            IntentData::new(Some(true), None)
        );
        // Escaped keys decode to the same key: order-free and escape-tolerant.
        assert_eq!(
            decode_ok("{\"\\u0076\":1,\"\\u0066loat\":false,\"stickyPrior\":null}"),
            IntentData::new(Some(false), None)
        );
        assert_eq!(
            decode_ok("{\"v\":1,\"float\":null,\"stickyPrior\":null}   "),
            IntentData::new(None, None)
        );
    }

    #[test]
    fn duplicate_decoded_keys_refuse_without_last_key_wins() {
        // Plain duplicate: the second value must not win.
        assert_eq!(
            decode_intent_data("{\"v\":1,\"float\":null,\"float\":true,\"stickyPrior\":null}"),
            Err(IntentDataError::DuplicateField)
        );
        // Escaped equivalent decodes to the same key, so it duplicates too.
        assert_eq!(
            decode_intent_data(
                "{\"v\":1,\"float\":null,\"\\u0066loat\":true,\"stickyPrior\":null}"
            ),
            Err(IntentDataError::DuplicateField)
        );
        assert_eq!(
            decode_intent_data("{\"v\":1,\"float\":null,\"stickyPrior\":null,\"\\u0076\":1}"),
            Err(IntentDataError::DuplicateField)
        );
    }

    #[test]
    fn missing_unknown_wrong_type_and_version_refuse() {
        assert_eq!(decode_intent_data("{}"), Err(IntentDataError::MissingField));
        assert_eq!(
            decode_intent_data("{\"v\":1,\"float\":null}"),
            Err(IntentDataError::MissingField)
        );
        assert_eq!(
            decode_intent_data("{\"v\":1,\"float\":null,\"stickyPrior\":null,\"extra\":1}"),
            Err(IntentDataError::UnknownField)
        );
        // Wrong types never coerce: numbers/strings/objects/arrays refuse.
        for bad in [
            "{\"v\":\"1\",\"float\":null,\"stickyPrior\":null}",
            "{\"v\":true,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":null,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":1,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":\"true\",\"stickyPrior\":null}",
            "{\"v\":1,\"float\":{},\"stickyPrior\":null}",
            "{\"v\":1,\"float\":[],\"stickyPrior\":null}",
            "{\"v\":1,\"float\":null,\"stickyPrior\":0}",
            "{\"v\":1,\"float\":null,\"stickyPrior\":\"false\"}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::UnexpectedType),
                "{bad}"
            );
        }
        // Well-formed non-1 versions refuse as versions, not values. The
        // version lane is integer-only: `1.0`/`1e0` never coerce to 1.
        for bad in [
            "{\"v\":2,\"float\":true,\"stickyPrior\":null}",
            "{\"v\":0,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1.0,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1e0,\"float\":null,\"stickyPrior\":null}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::UnsupportedVersion),
                "{bad}"
            );
        }
        // A number glued to garbage is malformed, never a version verdict:
        // `2x` refuses like supported-digit `1x`, while a cleanly
        // terminated `2` above stays a version.
        for bad in [
            "{\"v\":2x,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1x,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1e,\"float\":null,\"stickyPrior\":null}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::Malformed),
                "{bad}"
            );
        }
    }

    #[test]
    fn narrow_keys_literals_and_whitespace_refuse() {
        // Keys are case-sensitive, even when escaped: near-miss keys refuse
        // as unknown rather than decoding a neighboring lane.
        for bad in [
            "{\"V\":1,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1,\"Float\":null,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":null,\"STICKYPRIOR\":null}",
            "{\"\\u0056\":1,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1,\"\\u0046loat\":null,\"stickyPrior\":null}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::UnknownField),
                "{bad}"
            );
        }
        // Literals are lowercase-exact: uppercase spellings are wrong types,
        // while truncated or extended spellings are malformed JSON.
        for bad in [
            "{\"v\":1,\"float\":True,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":FALSE,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":null,\"stickyPrior\":Null}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::UnexpectedType),
                "{bad}"
            );
        }
        for bad in [
            "{\"v\":1,\"float\":truex,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":nul,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":null,\"stickyPrior\":tru}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::Malformed),
                "{bad}"
            );
        }
        // Only JSON whitespace (space/tab/LF/CR) is insignificant: vertical
        // tab, form feed, no-break space, and a BOM all refuse.
        for bad in [
            "\u{feff}{\"v\":1,\"float\":null,\"stickyPrior\":null}",
            "{\u{b}\"v\":1,\"float\":null,\"stickyPrior\":null}",
            "{\u{c}\"v\":1,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1,\u{a0}\"float\":null,\"stickyPrior\":null}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::Malformed),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn malformed_and_trailing_json_refuse() {
        for bad in [
            "",
            " ",
            "null",
            "[]",
            "\"x\"",
            "1",
            "{",
            "{\"v\":1,",
            "{\"v\":1,\"float\":null,\"stickyPrior\":null",
            "{\"v\":1,\"float\":null,\"stickyPrior\":null}}",
            "{\"v\":1,\"float\":null,\"stickyPrior\":null} trailing",
            "{\"v\":1,\"float\":null,\"stickyPrior\":null} {}",
            "{'v':1,'float':null,'stickyPrior':null}",
            "{\"v\":01,\"float\":null,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":null,\"stickyPrior\":null,}",
            "{\"v\":1 \"float\":null,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":nul,\"stickyPrior\":null}",
            "{\"v\":1,\"float\":null,\"stickyPrior\":null",
            "{\"\\uD800\":1,\"float\":null,\"stickyPrior\":null}",
            "{\"\\q\":1,\"float\":null,\"stickyPrior\":null}",
        ] {
            assert_eq!(
                decode_intent_data(bad),
                Err(IntentDataError::Malformed),
                "{bad}"
            );
        }
    }

    #[test]
    fn byte_bound_accepts_1024_and_rejects_1025() {
        let canonical = "{\"v\":1,\"float\":null,\"stickyPrior\":null}";
        assert!(canonical.len() < MAX_INTENT_DATA_BYTES);
        // Pad with insignificant JSON whitespace to the exact byte bound.
        let mut at_bound = String::from("{");
        at_bound.push_str(&" ".repeat(MAX_INTENT_DATA_BYTES - canonical.len()));
        at_bound.push_str(&canonical[1..]);
        assert_eq!(at_bound.len(), MAX_INTENT_DATA_BYTES);
        assert_eq!(decode_ok(&at_bound), IntentData::new(None, None));
        let over = format!("{at_bound} ");
        assert_eq!(over.len(), MAX_INTENT_DATA_BYTES + 1);
        assert_eq!(decode_intent_data(&over), Err(IntentDataError::Overbound));
        // The bound counts UTF-8 bytes, not characters: 600 two-byte
        // characters exceed 1024 bytes while staying far under 1024 chars.
        let wide = "\u{e9}".repeat(600);
        assert!(wide.chars().count() < MAX_INTENT_DATA_BYTES);
        assert!(wide.len() > MAX_INTENT_DATA_BYTES);
        assert_eq!(decode_intent_data(&wide), Err(IntentDataError::Overbound));
    }

    #[test]
    fn diagnostics_are_fixed_and_typed() {
        assert_eq!(IntentDataError::Overbound.as_str(), "overbound");
        assert_eq!(IntentDataError::Malformed.as_str(), "malformed");
        assert_eq!(IntentDataError::MissingField.as_str(), "missing-field");
        assert_eq!(IntentDataError::UnknownField.as_str(), "unknown-field");
        assert_eq!(IntentDataError::DuplicateField.as_str(), "duplicate-field");
        assert_eq!(IntentDataError::UnexpectedType.as_str(), "unexpected-type");
        assert_eq!(
            IntentDataError::UnsupportedVersion.as_str(),
            "unsupported-version"
        );
        for error in [
            IntentDataError::Overbound,
            IntentDataError::Malformed,
            IntentDataError::MissingField,
            IntentDataError::UnknownField,
            IntentDataError::DuplicateField,
            IntentDataError::UnexpectedType,
            IntentDataError::UnsupportedVersion,
        ] {
            assert!(!error.message().is_empty());
        }
    }
}
