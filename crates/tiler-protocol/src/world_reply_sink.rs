//! Output sinks for checked v4 reply and broker projection serialization.
//!
//! Supports zero-buffer canonical emission (`CountSink`, `StringSink`) in
//! contract table order with C0 escaping, and stack-buffered lexicographic
//! emission (`SortedStringSink`) with `serde_json`-compatible escaping.

use crate::world_protocol::push_json_string;

pub(crate) struct ObjectWriter<'a, S: ReplySink> {
    sink: &'a mut S,
    has_fields: bool,
}

impl<S: ReplySink> ObjectWriter<'_, S> {
    pub(crate) fn field<E, F>(&mut self, key: &'static str, write_val: F) -> Result<(), E>
    where
        F: FnOnce(&mut S) -> Result<(), E>,
    {
        if S::BUFFERED_FIELDS {
            let start = self.sink.begin_field();
            write_val(self.sink)?;
            self.sink.end_field(key, start);
        } else {
            if self.has_fields {
                self.sink.push_char(',');
            }
            self.has_fields = true;
            self.sink.push_char('"');
            self.sink.push_str(key);
            self.sink.push_str("\":");
            write_val(self.sink)?;
        }
        Ok(())
    }

    pub(crate) fn field_str<E>(&mut self, key: &'static str, val: &str) -> Result<(), E> {
        self.field(key, |s| {
            s.push_json_str(val);
            Ok(())
        })
    }

    pub(crate) fn field_bool<E>(&mut self, key: &'static str, val: bool) -> Result<(), E> {
        self.field(key, |s| {
            s.push_bool(val);
            Ok(())
        })
    }

    pub(crate) fn field_u64_str<E>(&mut self, key: &'static str, val: u64) -> Result<(), E> {
        self.field(key, |s| {
            s.push_char('"');
            s.push_u64(val);
            s.push_char('"');
            Ok(())
        })
    }

    pub(crate) fn field_opt_u64_str<E>(
        &mut self,
        key: &'static str,
        val: Option<u64>,
    ) -> Result<(), E> {
        self.field(key, |s| {
            match val {
                None => s.push_str("null"),
                Some(v) => {
                    s.push_char('"');
                    s.push_u64(v);
                    s.push_char('"');
                }
            }
            Ok(())
        })
    }

    pub(crate) fn field_raw<E>(&mut self, key: &'static str, raw: &'static str) -> Result<(), E> {
        self.field(key, |s| {
            s.push_str(raw);
            Ok(())
        })
    }

    pub(crate) fn field_array<T, E, F>(
        &mut self,
        key: &'static str,
        items: &[T],
        mut write_item: F,
    ) -> Result<(), E>
    where
        F: FnMut(&T, &mut S) -> Result<(), E>,
    {
        self.field(key, |s| {
            s.push_char('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    s.push_char(',');
                }
                write_item(item, s)?;
            }
            s.push_char(']');
            Ok(())
        })
    }
}

pub(crate) trait ReplySink: Sized {
    const BUFFERED_FIELDS: bool;

    fn push_str(&mut self, s: &str);
    fn push_char(&mut self, c: char);
    fn push_u64(&mut self, v: u64);
    fn push_i32(&mut self, v: i32);
    fn push_usize(&mut self, v: usize);
    fn push_bool(&mut self, v: bool);
    fn push_json_str(&mut self, s: &str);

    fn begin_field(&mut self) -> usize {
        0
    }

    fn end_field(&mut self, _key: &'static str, _start: usize) {}

    fn write_object<E, F>(&mut self, body: F) -> Result<(), E>
    where
        F: FnOnce(&mut ObjectWriter<'_, Self>) -> Result<(), E>;
}

pub(crate) struct CountSink(pub(crate) usize);

impl ReplySink for CountSink {
    const BUFFERED_FIELDS: bool = false;

    fn push_str(&mut self, s: &str) {
        self.0 += s.len();
    }

    fn push_char(&mut self, c: char) {
        self.0 += c.len_utf8();
    }

    fn push_u64(&mut self, v: u64) {
        self.0 += u64_decimal_len(v);
    }

    fn push_i32(&mut self, v: i32) {
        self.0 += i32_decimal_len(v);
    }

    fn push_usize(&mut self, v: usize) {
        self.0 += u64_decimal_len(v as u64);
    }

    fn push_bool(&mut self, v: bool) {
        self.0 += if v { 4 } else { 5 };
    }

    fn push_json_str(&mut self, s: &str) {
        self.0 += json_string_len(s);
    }

    fn write_object<E, F>(&mut self, body: F) -> Result<(), E>
    where
        F: FnOnce(&mut ObjectWriter<'_, Self>) -> Result<(), E>,
    {
        self.push_char('{');
        let mut writer = ObjectWriter {
            sink: self,
            has_fields: false,
        };
        body(&mut writer)?;
        self.push_char('}');
        Ok(())
    }
}

pub(crate) struct StringSink<'a>(pub(crate) &'a mut String);

impl ReplySink for StringSink<'_> {
    const BUFFERED_FIELDS: bool = false;

    fn push_str(&mut self, s: &str) {
        self.0.push_str(s);
    }

    fn push_char(&mut self, c: char) {
        self.0.push(c);
    }

    fn push_u64(&mut self, v: u64) {
        push_u64_decimal(self.0, v);
    }

    fn push_i32(&mut self, v: i32) {
        push_i32_decimal(self.0, v);
    }

    fn push_usize(&mut self, v: usize) {
        push_u64_decimal(self.0, v as u64);
    }

    fn push_bool(&mut self, v: bool) {
        self.0.push_str(if v { "true" } else { "false" });
    }

    fn push_json_str(&mut self, s: &str) {
        push_json_string(self.0, s);
    }

    fn write_object<E, F>(&mut self, body: F) -> Result<(), E>
    where
        F: FnOnce(&mut ObjectWriter<'_, Self>) -> Result<(), E>,
    {
        self.push_char('{');
        let mut writer = ObjectWriter {
            sink: self,
            has_fields: false,
        };
        body(&mut writer)?;
        self.push_char('}');
        Ok(())
    }
}

pub(crate) struct SortedStringSink {
    buffers: Vec<String>,
    depth: usize,
    fields: Vec<(&'static str, usize, usize)>,
}

impl SortedStringSink {
    pub(crate) fn new() -> Self {
        Self {
            buffers: vec![String::new()],
            depth: 0,
            fields: Vec::new(),
        }
    }

    pub(crate) fn finish(self) -> String {
        debug_assert_eq!(self.depth, 0);
        debug_assert!(self.fields.is_empty());
        self.buffers.into_iter().next().unwrap_or_default()
    }
}

impl ReplySink for SortedStringSink {
    const BUFFERED_FIELDS: bool = true;

    fn push_str(&mut self, s: &str) {
        self.buffers[self.depth].push_str(s);
    }

    fn push_char(&mut self, c: char) {
        self.buffers[self.depth].push(c);
    }

    fn push_u64(&mut self, v: u64) {
        push_u64_decimal(&mut self.buffers[self.depth], v);
    }

    fn push_i32(&mut self, v: i32) {
        push_i32_decimal(&mut self.buffers[self.depth], v);
    }

    fn push_usize(&mut self, v: usize) {
        push_u64_decimal(&mut self.buffers[self.depth], v as u64);
    }

    fn push_bool(&mut self, v: bool) {
        self.buffers[self.depth].push_str(if v { "true" } else { "false" });
    }

    fn push_json_str(&mut self, s: &str) {
        push_serde_json_string(&mut self.buffers[self.depth], s);
    }

    fn begin_field(&mut self) -> usize {
        self.buffers[self.depth].len()
    }

    fn end_field(&mut self, key: &'static str, start: usize) {
        let end = self.buffers[self.depth].len();
        self.fields.push((key, start, end));
    }

    fn write_object<E, F>(&mut self, body: F) -> Result<(), E>
    where
        F: FnOnce(&mut ObjectWriter<'_, Self>) -> Result<(), E>,
    {
        let field_base = self.fields.len();
        self.depth += 1;
        if self.depth >= self.buffers.len() {
            self.buffers.push(String::new());
        } else {
            self.buffers[self.depth].clear();
        }
        let mut writer = ObjectWriter {
            sink: self,
            has_fields: false,
        };
        let res = body(&mut writer);
        self.depth -= 1;
        if res.is_err() {
            self.fields.truncate(field_base);
            return res;
        }
        self.fields[field_base..].sort_unstable_by(|a, b| a.0.cmp(b.0));
        let (parent_slice, child_slice) = self.buffers.split_at_mut(self.depth + 1);
        let parent = &mut parent_slice[self.depth];
        let child = &child_slice[0];
        parent.push('{');
        for (i, &(key, start, end)) in self.fields[field_base..].iter().enumerate() {
            if i > 0 {
                parent.push(',');
            }
            parent.push('"');
            parent.push_str(key);
            parent.push_str("\":");
            parent.push_str(&child[start..end]);
        }
        parent.push('}');
        self.fields.truncate(field_base);
        Ok(())
    }
}

fn u64_decimal_len(mut v: u64) -> usize {
    let mut digits = 1;
    while v >= 10 {
        v /= 10;
        digits += 1;
    }
    digits
}

fn i32_decimal_len(v: i32) -> usize {
    u64_decimal_len(u64::from(v.unsigned_abs())) + usize::from(v < 0)
}

fn push_u64_decimal(out: &mut String, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut idx = buf.len();
    loop {
        idx -= 1;
        buf[idx] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    out.push_str(std::str::from_utf8(&buf[idx..]).expect("ASCII decimal digits are valid UTF-8"));
}

fn push_i32_decimal(out: &mut String, v: i32) {
    if v < 0 {
        out.push('-');
    }
    push_u64_decimal(out, u64::from(v.unsigned_abs()));
}

fn json_string_len(value: &str) -> usize {
    let mut bytes = 2usize;
    for byte in value.bytes() {
        bytes += match byte {
            b'"' | b'\\' => 2,
            0x00..=0x1F => 6,
            _ => 1,
        };
    }
    bytes
}

fn push_serde_json_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0'..='\u{1f}' => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let byte = c as u8;
                out.push_str("\\u00");
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::push_serde_json_string;

    #[test]
    fn serde_json_string_escaping_matches_serde_json() {
        let mut all_ascii = String::new();
        for b in 0u8..=0x7F {
            all_ascii.push(char::from(b));
        }
        all_ascii.push_str("alpha-\u{00e9}-\u{2603}-\u{1f600}");
        let mut ours = String::new();
        push_serde_json_string(&mut ours, &all_ascii);
        let expected = serde_json::to_string(&all_ascii).expect("serde_json string");
        assert_eq!(ours, expected);
    }
}
