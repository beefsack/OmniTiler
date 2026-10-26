//! Workspace `unsafe_code` allowlist gate (runs on Linux, Windows, and macOS).
//!
//! Enforcement, fail-closed (every failure names the file and the fix):
//! 1. The workspace root carries `[workspace.lints.rust] unsafe_code = "forbid"`.
//! 2. Every workspace member declares an explicit lint posture, so a new crate
//!    root cannot silently opt out:
//!    - ordinary crates: exactly `[lints] workspace = true` (inherits forbid);
//!    - `tiler-windows` / `tiler-macos` / `tiler-kwin-effect-ffi`: exactly
//!      `[lints.rust] unsafe_code = "deny"` with no workspace inheritance
//!      (Cargo cannot inherit workspace lints and override one key, so the
//!      exception stands alone; `deny`, not `forbid`, is load-bearing because
//!      `forbid` at the crate root would pin the whole crate and the approved
//!      boundary `allow` below could no longer opt back in). A crate with no
//!      lint posture at all fails, so a new crate cannot bypass the gate.
//! 3. Every member `.rs` file is scanned for `unsafe_code` weakening (`allow`,
//!    `warn`, `expect`, `force_warn`, including multiline spellings, combined
//!    lint lists, and `cfg_attr` wrappers). Only `allow` inside a file listed
//!    in `APPROVED_ALLOW_ROOTS` passes; `deny`/`forbid` strengthening passes
//!    anywhere. Real `unsafe` blocks are the compiler's job (deny/forbid
//!    rejects them); this gate owns the lint attributes that would silence it.
//!
//! Approved `allow(unsafe_code)` roots (the only ones):
//! - `crates/tiler-windows/src/ffi/mod.rs` (child modules inherit the allow)
//! - `crates/tiler-macos/src/ffi.rs`
//! - `crates/tiler-kwin-effect-ffi/src/drag_oracle.rs` (native ABI boundary;
//!   the retired group-highlight and visual-policy modules were deleted
//!   with their native consumers in Stage 5, so no `unsafe` boundary
//!   remains there and their allowlist roots are removed, not retained)
//!
//! Run: `cargo test --locked -p tiler-core --test unsafe_guardrail`
//! (also runs inside `cargo test --workspace`). No dependencies beyond `std`.
//!
//! Restricted manifest syntax (deliberate, documented): the parser understands
//! the style used in this repo - `#` comments, `[dotted.sections]`,
//! `key = true`, `key = "string"` / `key = 'string'`, and flat multiline
//! `key = [ "a", "b" ]` string arrays. Anything else in a lint-bearing or
//! workspace section (triple-quoted strings, inline tables, unexpected lint
//! tables, extra keys) is a hard error naming file and line, so an
//! unsupported spelling can never read as approval. `[[bin]]`-style headers
//! outside lint/workspace sections are accepted and ignored.
//!
//! Source-scanner limits (documented): the tokenizer strips `//` and nested
//! `/* */` comments, plain/byte/C strings, raw strings of any hash count, and
//! char literals (with a lifetime-vs-literal heuristic), then matches balanced
//! `[...]` attribute bodies. Raw identifiers (`r#allow`) are recognized. An
//! unbalanced attribute or unterminated string/comment is a hard error, never
//! a silent pass.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Workspace-relative (`/`) files permitted to carry `allow(unsafe_code)`.
const APPROVED_ALLOW_ROOTS: &[&str] = &[
    "crates/tiler-windows/src/ffi/mod.rs",
    "crates/tiler-macos/src/ffi.rs",
    "crates/tiler-kwin-effect-ffi/src/drag_oracle.rs",
    "crates/tiler-live-fixture/src/pipe_win.rs",
];

/// Crates allowed a standalone `[lints.rust] unsafe_code = "deny"`.
const DENY_EXCEPTION_CRATES: &[&str] = &[
    "tiler-kwin-effect-ffi",
    "tiler-macos",
    "tiler-windows",
    "tiler-live-fixture",
];

/// Lint levels that weaken `unsafe_code`. `deny`/`forbid` strengthen it and
/// are accepted anywhere. `force_warn` is not stable attribute syntax today;
/// it is listed so a future spelling fails here instead of slipping through.
const WEAKENING_LEVELS: &[&str] = &["allow", "expect", "force_warn", "warn"];

/// One `unsafe_code` weakening found in a source file.
struct Finding {
    line: usize,
    level: String,
    inner: bool,
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(2)
        .expect("tiler-core lives at <root>/crates/tiler-core")
        .to_path_buf()
}

/// Workspace-relative path with `/` separators for allowlist comparison.
fn rel_slash(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn line_of(text: &str, idx: usize) -> usize {
    text[..idx].bytes().filter(|b| *b == b'\n').count() + 1
}

// ---------- source tokenizer ----------

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn push_blank(out: &mut String, span: &str) {
    for b in span.as_bytes() {
        out.push(if *b == b'\n' { '\n' } else { ' ' });
    }
}

/// Strip comments and string/char literals, blanking them (newlines kept) so
/// only real code - including attributes - remains.
fn strip_code(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let b = rest.as_bytes();
        if b.starts_with(b"//") {
            let end = rest.find('\n').map(|o| i + o).unwrap_or(text.len());
            push_blank(&mut out, &text[i..end]);
            i = end;
        } else if b.starts_with(b"/*") {
            let mut depth = 0usize;
            let mut j = i;
            while j < text.len() {
                let r = &text[j..];
                if r.as_bytes().starts_with(b"/*") {
                    depth += 1;
                    j += 2;
                } else if r.as_bytes().starts_with(b"*/") {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += r.chars().next().expect("non-empty").len_utf8();
                }
            }
            if depth != 0 {
                return Err(format!(
                    "unterminated block comment at line {}",
                    line_of(text, i)
                ));
            }
            push_blank(&mut out, &text[i..j]);
            i = j;
        } else if let Some((opener_len, hashes)) = raw_opener(rest) {
            let closer = format!("\"{}", "#".repeat(hashes));
            let body = &rest[opener_len..];
            match body.find(&closer) {
                Some(o) => {
                    let end = opener_len + o + closer.len();
                    push_blank(&mut out, &rest[..end]);
                    i += end;
                }
                None => {
                    return Err(format!(
                        "unterminated raw string at line {}",
                        line_of(text, i)
                    ));
                }
            }
        } else if b[0] == b'"'
            || ((b[0] == b'b' || b[0] == b'B' || b[0] == b'c' || b[0] == b'C')
                && b.get(1) == Some(&b'"'))
        {
            let mut j = if b[0] == b'"' { 1 } else { 2 };
            let mut closed = false;
            while j < b.len() {
                if b[j] == b'\\' {
                    j += 2; // escape pair, including backslash-newline continuation
                } else if b[j] == b'"' {
                    j += 1;
                    closed = true;
                    break;
                } else {
                    // Ordinary content, including raw newlines: `"..."` literals
                    // may span lines in compilable Rust, so only EOF ends one.
                    j += 1;
                }
            }
            if !closed {
                return Err(format!("unterminated string at line {}", line_of(text, i)));
            }
            push_blank(&mut out, &rest[..j]);
            i += j;
        } else if b[0] == b'\'' {
            match parse_char_literal(rest) {
                Some(len) => {
                    push_blank(&mut out, &rest[..len]);
                    i += len;
                }
                // Not a literal (e.g. a lifetime `'a`): leave the quote for
                // rustc. Anything invalid here cannot compile anyway.
                None => {
                    out.push('\'');
                    i += 1;
                }
            }
        } else {
            let ch = rest.chars().next().expect("non-empty");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    Ok(out)
}

/// Byte length of the char/byte literal opening at `rest` (at the `'`), or
/// `None` for non-literals (e.g. lifetimes). Accepts exactly the shapes valid
/// in compilable Rust, so a missed parse can never hide a real attribute.
fn parse_char_literal(rest: &str) -> Option<usize> {
    let b = rest.as_bytes();
    if b.first() != Some(&b'\'') {
        return None;
    }
    let after = &b[1..];
    if after.first() == Some(&b'\\') {
        // Escape (`'\n'`, `'\''`, `'\x7F'`, `'\u{1F600}'`): the closing quote
        // follows within a few bytes on the same line. Lifetimes never contain
        // a backslash, so this shape is unambiguous in code rustc accepts.
        let mut d = 1;
        while d <= 10 && after.get(d).is_some_and(|c| *c != b'\'' && *c != b'\n') {
            d += 1;
        }
        if after.get(d) == Some(&b'\'') {
            Some(1 + d + 1)
        } else {
            None
        }
    } else {
        let ch = rest[1..].chars().next()?;
        if ch == '\'' || ch == '\n' {
            return None;
        }
        let end = 1 + ch.len_utf8();
        if b.get(end) == Some(&b'\'') {
            Some(end + 1)
        } else {
            None
        }
    }
}

/// Raw-string opener (`r"..."`, `br#"..."#`, `cr"..."`): opener length and hash count.
fn raw_opener(rest: &str) -> Option<(usize, usize)> {
    let b = rest.as_bytes();
    if b.is_empty() {
        return None;
    }
    let mut j = if (b[0] == b'b' || b[0] == b'B' || b[0] == b'c' || b[0] == b'C')
        && b.get(1) == Some(&b'r')
    {
        2
    } else if b[0] == b'r' {
        1
    } else {
        return None;
    };
    let mut hashes = 0;
    while b.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    if b.get(j) == Some(&b'"') {
        Some((j + 1, hashes))
    } else {
        None
    }
}

/// Extract `(byte index of '#', is_inner, body)` for every attribute.
fn attribute_bodies(cleaned: &str) -> Result<Vec<(usize, bool, String)>, String> {
    let b = cleaned.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'#' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while b.get(j).is_some_and(|c| c.is_ascii_whitespace()) {
            j += 1;
        }
        let mut inner = false;
        if b.get(j) == Some(&b'!') {
            inner = true;
            j += 1;
            while b.get(j).is_some_and(|c| c.is_ascii_whitespace()) {
                j += 1;
            }
        }
        if b.get(j) != Some(&b'[') {
            i += 1; // `#` outside an attribute cannot appear in valid Rust
            continue;
        }
        let mut depth = 0usize;
        let mut k = j;
        while k < b.len() {
            if b[k] == b'[' {
                depth += 1;
            } else if b[k] == b']' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            k += 1;
        }
        if depth != 0 {
            return Err(format!(
                "unbalanced attribute brackets at line {}",
                line_of(cleaned, i)
            ));
        }
        out.push((i, inner, cleaned[j + 1..k].to_string()));
        i = k + 1;
    }
    Ok(out)
}

/// Read an identifier (with optional `r#` prefix) at `i`; returns the plain
/// word and the end index.
fn read_ident(body: &str, i: usize) -> Option<(&str, usize)> {
    let b = body.as_bytes();
    let mut start = i;
    if b.get(i) == Some(&b'r')
        && b.get(i + 1) == Some(&b'#')
        && b.get(i + 2).is_some_and(|c| is_ident_start(*c))
    {
        start = i + 2;
    } else if !b.get(i).is_some_and(|c| is_ident_start(*c)) {
        return None;
    }
    let mut j = start + 1;
    while b.get(j).is_some_and(|c| is_ident_continue(*c)) {
        j += 1;
    }
    Some((&body[start..j], j))
}

fn match_paren(body: &str, open: usize) -> Option<usize> {
    let b = body.as_bytes();
    let mut depth = 0usize;
    let mut k = open;
    while k < b.len() {
        if b[k] == b'(' {
            depth += 1;
        } else if b[k] == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
        k += 1;
    }
    None
}

fn span_names_unsafe_code(span: &str) -> bool {
    let mut i = 0;
    while i < span.len() {
        match read_ident(span, i) {
            Some((word, j)) => {
                if word == "unsafe_code" {
                    return true;
                }
                i = j;
            }
            None => {
                i += 1;
            }
        }
    }
    false
}

/// Lint levels in `body` whose list gates `unsafe_code`.
fn body_weakening(body: &str) -> Vec<String> {
    let mut hits = Vec::new();
    let mut i = 0;
    while i < body.len() {
        match read_ident(body, i) {
            Some((word, j)) => {
                let mut k = j;
                while body
                    .as_bytes()
                    .get(k)
                    .is_some_and(|c| c.is_ascii_whitespace())
                {
                    k += 1;
                }
                if body.as_bytes().get(k) == Some(&b'(') && WEAKENING_LEVELS.contains(&word) {
                    if let Some(close) = match_paren(body, k) {
                        if span_names_unsafe_code(&body[k + 1..close]) {
                            hits.push(word.to_string());
                        }
                        i = close + 1;
                    } else {
                        // Unbalanced parens cannot compile; keep scanning so a
                        // later real attribute on the line is still checked.
                        i = k + 1;
                    }
                } else {
                    i = j;
                }
            }
            None => {
                i += 1;
            }
        }
    }
    hits
}

fn scan_source_text(text: &str) -> Result<Vec<(usize, Finding)>, String> {
    let cleaned = strip_code(text)?;
    let mut findings = Vec::new();
    for (hash_idx, inner, body) in attribute_bodies(&cleaned)? {
        for level in body_weakening(&body) {
            findings.push((
                hash_idx,
                Finding {
                    line: line_of(&cleaned, hash_idx),
                    level,
                    inner,
                },
            ));
        }
    }
    Ok(findings)
}

// ---------- restricted manifest parser ----------

struct Entry {
    key: String,
    value: TomlValue,
    line: usize,
}

enum TomlValue {
    Bool(bool),
    Str(String),
    Array(Vec<ArrayItem>),
    Raw(String),
}

impl TomlValue {
    fn describe(&self) -> String {
        match self {
            TomlValue::Bool(v) => v.to_string(),
            TomlValue::Str(s) => format!("\"{s}\""),
            TomlValue::Array(items) => format!("[{} item(s)]", items.len()),
            TomlValue::Raw(s) => format!("`{s}` (unsupported spelling)"),
        }
    }
}

struct ArrayItem {
    value: String,
}

struct Section {
    name: String,
    line: usize,
    entries: Vec<Entry>,
}

fn strip_toml_comment(line: &str, lineno: usize, origin: &str) -> Result<String, String> {
    if line.contains("\"\"\"") || line.contains("'''") {
        return Err(format!(
            "{origin}:{lineno}: triple-quoted strings are outside the gate's restricted TOML syntax; rewrite without them"
        ));
    }
    let b = line.as_bytes();
    let mut in_double = false;
    let mut in_single = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_double {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_double = false;
            }
            i += 1;
        } else if in_single {
            if c == b'\'' {
                in_single = false;
            }
            i += 1;
        } else if c == b'"' {
            in_double = true;
            i += 1;
        } else if c == b'\'' {
            in_single = true;
            i += 1;
        } else if c == b'#' {
            return Ok(line[..i].to_string());
        } else {
            i += 1;
        }
    }
    Ok(line.to_string())
}

fn valid_section_name(name: &str) -> bool {
    // Covers `[lints]`, `[workspace.lints.rust]`, and platform target tables
    // such as `[target.'cfg(windows)'.dependencies]`. Anything outside this
    // charset is rejected rather than silently accepted.
    !name.is_empty()
        && name.bytes().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    b'_' | b'-' | b'.' | b'\'' | b'"' | b'(' | b')' | b'=' | b' ' | b','
                )
        })
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// Quote-aware top-level bracket balance for array accumulation.
fn brackets_balanced(text: &str) -> bool {
    let b = text.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = 0;
    while i < b.len() {
        if in_str {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == b'"' {
                in_str = false;
            }
            i += 1;
        } else if b[i] == b'"' {
            in_str = true;
            i += 1;
        } else if b[i] == b'[' {
            depth += 1;
            i += 1;
        } else if b[i] == b']' {
            depth -= 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    !in_str && depth == 0
}

fn parse_scalar(raw: &str) -> TomlValue {
    if raw == "true" {
        TomlValue::Bool(true)
    } else if raw == "false" {
        TomlValue::Bool(false)
    } else if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        TomlValue::Str(raw[1..raw.len() - 1].to_string())
    } else {
        TomlValue::Raw(raw.to_string())
    }
}

fn parse_array_items(buffer: &str, origin: &str, lineno: usize) -> Result<Vec<ArrayItem>, String> {
    let fail = |why: &str| {
        format!(
            "{origin}:{lineno}: malformed string array ({why}); keep `members` a flat list of \"path\" strings"
        )
    };
    let t = buffer.trim();
    if !t.starts_with('[') || !t.ends_with(']') {
        return Err(fail("must start with `[` and end with `]`"));
    }
    let inner = &t[1..t.len() - 1];
    if inner.contains('[') || inner.contains(']') {
        return Err(fail("nested arrays are unsupported"));
    }
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_str = false;
    let chars: Vec<char> = inner.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            if c == '\\' {
                return Err(fail("escape sequences are unsupported"));
            }
            current.push(c);
            if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            current.push(c);
        } else if c == ',' {
            let item = current.trim().to_string();
            if !item.is_empty() {
                items.push(item);
            }
            current = String::new();
        } else {
            current.push(c);
        }
        i += 1;
    }
    if in_str {
        return Err(fail("unterminated string"));
    }
    let tail = current.trim().to_string();
    if !tail.is_empty() {
        items.push(tail);
    }
    let mut out = Vec::new();
    for item in items {
        if item.len() < 2 || !item.starts_with('"') || !item.ends_with('"') {
            return Err(fail(&format!("{item} is not a \"quoted\" string")));
        }
        out.push(ArrayItem {
            value: item[1..item.len() - 1].to_string(),
        });
    }
    Ok(out)
}

fn parse_manifest(text: &str, origin: &str) -> Result<Vec<Section>, String> {
    let mut sections: Vec<Section> = Vec::new();
    let mut cur: Option<usize> = None;
    // (section index, key, first line, accumulated buffer)
    let mut pending_array: Option<(usize, String, usize, String)> = None;
    for (idx, raw_line) in text.lines().enumerate() {
        let lineno = idx + 1;
        let stripped = strip_toml_comment(raw_line, lineno, origin)?;
        let t = stripped.trim().to_string();
        if let Some((s, key, start_line, mut buf)) = pending_array.take() {
            if !t.is_empty() {
                buf.push(' ');
                buf.push_str(&t);
            }
            if brackets_balanced(&buf) {
                let items = parse_array_items(&buf, origin, start_line)?;
                sections[s].entries.push(Entry {
                    key,
                    value: TomlValue::Array(items),
                    line: start_line,
                });
            } else {
                pending_array = Some((s, key, start_line, buf));
            }
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if t.starts_with('[') {
            let name = if t.starts_with("[[") {
                t.strip_suffix("]]")
                    .ok_or_else(|| format!("{origin}:{lineno}: malformed `[[section]]` header"))?
                    .strip_prefix("[[")
                    .expect("prefix checked")
            } else {
                t.strip_suffix(']')
                    .ok_or_else(|| format!("{origin}:{lineno}: malformed `[section]` header"))?
                    .strip_prefix('[')
                    .expect("prefix checked")
            };
            if !valid_section_name(name) {
                return Err(format!(
                    "{origin}:{lineno}: unsupported section name [{name}]"
                ));
            }
            sections.push(Section {
                name: name.to_string(),
                line: lineno,
                entries: Vec::new(),
            });
            cur = Some(sections.len() - 1);
            continue;
        }
        let s = cur.ok_or_else(|| format!("{origin}:{lineno}: key-value outside any section"))?;
        let (raw_key, raw_value) = t
            .split_once('=')
            .ok_or_else(|| format!("{origin}:{lineno}: expected `key = value`"))?;
        let key = raw_key.trim();
        if !valid_key(key) {
            return Err(format!("{origin}:{lineno}: unsupported key `{key}`"));
        }
        let value = raw_value.trim().to_string();
        if value.starts_with('[') {
            if brackets_balanced(&value) {
                let items = parse_array_items(&value, origin, lineno)?;
                sections[s].entries.push(Entry {
                    key: key.to_string(),
                    value: TomlValue::Array(items),
                    line: lineno,
                });
            } else {
                pending_array = Some((s, key.to_string(), lineno, value));
            }
        } else {
            sections[s].entries.push(Entry {
                key: key.to_string(),
                value: parse_scalar(&value),
                line: lineno,
            });
        }
    }
    if let Some((_, key, start_line, _)) = pending_array {
        return Err(format!(
            "{origin}:{start_line}: unterminated array for `{key}`"
        ));
    }
    Ok(sections)
}

fn is_str_forbid(value: &TomlValue) -> bool {
    matches!(value, TomlValue::Str(s) if s == "forbid")
}

fn is_str_deny(value: &TomlValue) -> bool {
    matches!(value, TomlValue::Str(s) if s == "deny")
}

/// Validate the workspace root manifest; returns the member list.
fn check_root_manifest(
    sections: &[Section],
    origin: &str,
    errors: &mut Vec<String>,
) -> Vec<String> {
    let mut members = Vec::new();
    let mut saw_workspace = false;
    let mut saw_workspace_forbid = false;
    for section in sections {
        if section.name == "workspace" {
            saw_workspace = true;
            for entry in &section.entries {
                if entry.key == "members" {
                    match &entry.value {
                        TomlValue::Array(items) => {
                            for item in items {
                                if item.value.is_empty() {
                                    errors.push(format!(
                                        "{origin}:{}: empty path in `members`; list each crate directory explicitly",
                                        entry.line
                                    ));
                                } else {
                                    members.push(item.value.clone());
                                }
                            }
                        }
                        _ => errors.push(format!(
                            "{origin}:{}: `members` must be a string array; keep the flat `[ \"path\", ... ]` style",
                            entry.line
                        )),
                    }
                }
            }
        } else if section.name == "workspace.lints.rust" {
            if section.entries.len() == 1
                && section.entries[0].key == "unsafe_code"
                && is_str_forbid(&section.entries[0].value)
            {
                saw_workspace_forbid = true;
            } else {
                errors.push(format!(
                    "{origin}:{}: `[workspace.lints.rust]` must contain exactly `unsafe_code = \"forbid\"`",
                    section.line
                ));
            }
        } else if section.name == "workspace.lints" || section.name.starts_with("workspace.lints.")
        {
            errors.push(format!(
                "{origin}:{}: unexpected `[{}]`; the only workspace lint table is `[workspace.lints.rust]` with `unsafe_code = \"forbid\"`",
                section.line, section.name
            ));
        } else if section.name == "workspace" || section.name.starts_with("workspace.") {
            errors.push(format!(
                "{origin}:{}: unexpected `[{}]`; the gate only understands `[workspace]` (members) and `[workspace.lints.rust]`",
                section.line, section.name
            ));
        } else if section.name == "lints" || section.name.starts_with("lints.") {
            errors.push(format!(
                "{origin}:{}: unexpected `[{}]` in the workspace root; crate lint posture belongs in member manifests",
                section.line, section.name
            ));
        }
    }
    if !saw_workspace {
        errors.push(format!(
            "{origin}: missing `[workspace]` with a `members` list"
        ));
    }
    if members.is_empty() && errors.is_empty() {
        errors.push(format!(
            "{origin}: `[workspace]` `members` is empty; list every crate explicitly"
        ));
    }
    if !saw_workspace_forbid {
        errors.push(format!(
            "{origin}: missing `[workspace.lints.rust]` with exactly `unsafe_code = \"forbid\"`"
        ));
    }
    members
}

/// Validate one member manifest's lint posture.
fn check_member_manifest(
    crate_name: &str,
    sections: &[Section],
    origin: &str,
    errors: &mut Vec<String>,
) {
    let mut saw_package = false;
    for section in sections {
        if section.name == "package" {
            saw_package = true;
        }
        if section.name == "workspace" || section.name.starts_with("workspace.") {
            errors.push(format!(
                "{origin}:{}: member crates must not define `[{}]`; workspace tables live in the root manifest",
                section.line, section.name
            ));
        }
    }
    if !saw_package {
        errors.push(format!("{origin}: missing `[package]` with a `name`"));
        return;
    }
    let lint_sections: Vec<&Section> = sections
        .iter()
        .filter(|s| s.name == "lints" || s.name.starts_with("lints."))
        .collect();
    if lint_sections.is_empty() {
        errors.push(format!(
            "{origin}: `{crate_name}` declares no lint posture; add `[lints]\nworkspace = true` \
             (or, for a new FFI boundary crate, register a `[lints.rust]` exception in this gate first)"
        ));
        return;
    }
    if lint_sections.len() == 1 && lint_sections[0].name == "lints" {
        let section = lint_sections[0];
        if section.entries.len() == 1
            && section.entries[0].key == "workspace"
            && matches!(section.entries[0].value, TomlValue::Bool(true))
        {
            return; // ordinary crate: inherits the workspace forbid
        }
        errors.push(format!(
            "{origin}:{}: `[lints]` must contain exactly `workspace = true` (found `{}` = {}); per-lint overrides cannot be combined with inheritance",
            section.line, section.entries[0].key, section.entries[0].value.describe()
        ));
        return;
    }
    if lint_sections.len() == 1 && lint_sections[0].name == "lints.rust" {
        let section = lint_sections[0];
        if section.entries.len() == 1
            && section.entries[0].key == "unsafe_code"
            && is_str_deny(&section.entries[0].value)
        {
            if DENY_EXCEPTION_CRATES.contains(&crate_name) {
                return; // FFI exception crate: deny, boundary modules allow back in
            }
            errors.push(format!(
                "{origin}:{}: `{crate_name}` is not a registered `[lints.rust]` exception; inherit with `[lints]\nworkspace = true` instead",
                section.line
            ));
            return;
        }
        errors.push(format!(
            "{origin}:{}: `[lints.rust]` must contain exactly `unsafe_code = \"deny\"` (found `{}` = {}); any other unsafe-lint weakening is rejected",
            section.line, section.entries[0].key, section.entries[0].value.describe()
        ));
        return;
    }
    let names: Vec<&str> = lint_sections.iter().map(|s| s.name.as_str()).collect();
    errors.push(format!(
        "{origin}: unexpected lint sections [{}]; ordinary crates use `[lints] workspace = true`, \
         FFI exception crates use a lone `[lints.rust] unsafe_code = \"deny\"`",
        names.join(", ")
    ));
}

fn member_crate_name(
    sections: &[Section],
    origin: &str,
    errors: &mut Vec<String>,
) -> Option<String> {
    for section in sections {
        if section.name == "package" {
            for entry in &section.entries {
                if entry.key == "name" {
                    if let TomlValue::Str(name) = &entry.value {
                        return Some(name.clone());
                    }
                    errors.push(format!(
                        "{origin}:{}: `[package] name` must be a string",
                        entry.line
                    ));
                    return None;
                }
            }
            errors.push(format!(
                "{origin}:{}: `[package]` is missing `name`",
                section.line
            ));
            return None;
        }
    }
    None
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push(format!("cannot list {}: {e}", dir.display()));
            return;
        }
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        match entry {
            Ok(e) => paths.push(e.path()),
            Err(e) => errors.push(format!("cannot read entry in {}: {e}", dir.display())),
        }
    }
    paths.sort();
    for path in paths {
        let is_dir = !path.is_symlink() && path.is_dir();
        if is_dir {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rs_files(&path, out, errors);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Full gate over the live tree; returns every violation found.
fn check_workspace(root: &Path) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    let root_manifest_path = root.join("Cargo.toml");
    let root_text = match fs::read_to_string(&root_manifest_path) {
        Ok(text) => text,
        Err(e) => return vec![format!("cannot read {}: {e}", root_manifest_path.display())],
    };
    let root_origin = "Cargo.toml";
    let root_sections = match parse_manifest(&root_text, root_origin) {
        Ok(sections) => sections,
        Err(e) => return vec![e],
    };
    let members = check_root_manifest(&root_sections, root_origin, &mut errors);

    // A crate directory on disk that is not a declared member fails here
    // instead of silently skipping the gate.
    let crates_dir = root.join("crates");
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        let mut on_disk: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let manifest = entry.path().join("Cargo.toml");
            if manifest.is_file()
                && let Some(name) = entry.path().file_name().and_then(|n| n.to_str())
            {
                on_disk.push(format!("crates/{name}"));
            }
        }
        on_disk.sort();
        for dir in &on_disk {
            if !members.contains(dir) {
                errors.push(format!(
                    "{dir}/Cargo.toml exists on disk but is not in workspace `members`; add it and declare a lint posture"
                ));
            }
        }
    }

    let mut allow_seen = vec![false; APPROVED_ALLOW_ROOTS.len()];
    for member in &members {
        let manifest_path = root.join(member).join("Cargo.toml");
        let origin = rel_slash(root, &manifest_path);
        let text = match fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(e) => {
                errors.push(format!(
                    "{origin}: listed in workspace `members` but unreadable: {e}"
                ));
                continue;
            }
        };
        let sections = match parse_manifest(&text, &origin) {
            Ok(sections) => sections,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        let Some(crate_name) = member_crate_name(&sections, &origin, &mut errors) else {
            continue;
        };
        check_member_manifest(&crate_name, &sections, &origin, &mut errors);

        let mut files = Vec::new();
        collect_rs_files(&root.join(member), &mut files, &mut errors);
        for file in files {
            let rel = rel_slash(root, &file);
            let text = match fs::read_to_string(&file) {
                Ok(text) => text,
                Err(e) => {
                    errors.push(format!("{rel}: cannot read source: {e}"));
                    continue;
                }
            };
            let findings = match scan_source_text(&text) {
                Ok(findings) => findings,
                Err(e) => {
                    errors.push(format!(
                        "{rel}: cannot tokenize ({e}); fix the source so the gate can verify it"
                    ));
                    continue;
                }
            };
            for (_, finding) in findings {
                let approved_idx = APPROVED_ALLOW_ROOTS.iter().position(|r| *r == rel);
                if finding.level == "allow"
                    && let Some(idx) = approved_idx
                {
                    allow_seen[idx] = true;
                    continue;
                }
                let shape = if finding.inner { "#![…]" } else { "#[…]" };
                errors.push(format!(
                    "{rel}:{}: `{}` attribute weakens `unsafe_code` via `{}`; only {} may `allow(unsafe_code)`",
                    finding.line,
                    shape,
                    finding.level,
                    APPROVED_ALLOW_ROOTS.join(" and ")
                ));
            }
        }
    }

    for (idx, root_rel) in APPROVED_ALLOW_ROOTS.iter().enumerate() {
        if !root.join(root_rel).is_file() {
            errors.push(format!(
                "{root_rel}: approved boundary root is missing; restore the file or remove it from the allowlist in crates/tiler-core/tests/unsafe_guardrail.rs"
            ));
        } else if !allow_seen[idx] {
            errors.push(format!(
                "{root_rel}: approved boundary root no longer carries `allow(unsafe_code)`; restore it or remove the file from the allowlist"
            ));
        }
    }
    errors.sort();
    errors
}

#[test]
fn workspace_unsafe_posture() {
    let root = workspace_root();
    let errors = check_workspace(&root);
    assert!(
        errors.is_empty(),
        "unsafe allowlist gate failed with {} violation(s):\n{}",
        errors.len(),
        errors
            .iter()
            .take(40)
            .map(|e| format!("- {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[cfg(test)]
mod scanner_tests {
    use super::scan_source_text;

    fn levels(source: &str) -> Vec<String> {
        scan_source_text(source)
            .expect("fixture must tokenize")
            .into_iter()
            .map(|(_, f)| format!("{}:{}", if f.inner { "inner" } else { "outer" }, f.level))
            .collect()
    }

    fn expect(source: &str, want: &[&str]) {
        assert_eq!(levels(source), want, "{source}");
    }

    fn expect_clean(sources: &[&str]) {
        for source in sources {
            assert!(levels(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn approved_root_allow_passes() {
        expect("#![allow(unsafe_code)]\nfn f() {}", &["inner:allow"]);
        expect("#[allow(unsafe_code)]\nfn f() {}", &["outer:allow"]);
    }

    #[test]
    fn stray_outer_allow_rejected() {
        expect("#[allow(unsafe_code)]", &["outer:allow"]);
    }

    #[test]
    fn stray_inner_allow_rejected() {
        expect("#![allow(unsafe_code)]", &["inner:allow"]);
    }

    #[test]
    fn multiline_and_whitespace_variants_found() {
        for (source, want) in [
            ("#![allow(\n    unsafe_code,\n)]", "inner:allow"),
            ("#[allow ( unsafe_code )]", "outer:allow"),
            ("#[allow\t(\nunsafe_code\n)]", "outer:allow"),
        ] {
            expect(source, &[want]);
        }
    }

    #[test]
    fn combined_lists_found() {
        for source in [
            "#![allow(clippy::too_many_arguments, unsafe_code)]",
            "#[allow(unsafe_code, clippy::all)]",
            "#[allow(rustfmt::skip, unsafe_code)]",
        ] {
            assert_eq!(levels(source).len(), 1, "{source}");
            assert!(levels(source)[0].ends_with(":allow"), "{source}");
        }
    }

    #[test]
    fn cfg_attr_wrappers_found() {
        expect("#[cfg_attr(windows, allow(unsafe_code))]", &["outer:allow"]);
        expect(
            "#![cfg_attr(all(), deny(warnings), allow(unsafe_code))]",
            &["inner:allow"],
        );
        expect(
            "#[cfg_attr(unix, cfg_attr(target_os = \"linux\", allow(unsafe_code)))]",
            &["outer:allow"],
        );
    }

    #[test]
    fn warn_expect_and_force_warn_found() {
        for (source, want) in [
            ("#![warn(unsafe_code)]", "inner:warn"),
            ("#[expect(unsafe_code)]", "outer:expect"),
            ("#[force_warn(unsafe_code)]", "outer:force_warn"),
            ("#[cfg_attr(test, expect(unsafe_code))]", "outer:expect"),
        ] {
            expect(source, &[want]);
        }
    }

    #[test]
    fn deny_and_forbid_strengthening_ignored() {
        expect_clean(&[
            "#![deny(unsafe_code)]",
            "#![forbid(unsafe_code)]",
            "#[deny(unsafe_code)]",
        ]);
    }

    #[test]
    fn unsafe_adjacent_non_lint_syntax_ignored() {
        // Real `unsafe` code without a lint attribute is the compiler's job,
        // not this gate's; names merely containing "unsafe" must not match.
        expect_clean(&[
            "fn f() { unsafe { g(); } }",
            "#[unsafe(no_mangle)]\nextern \"C\" fn f() {}",
            "#![allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]",
            "unsafe extern \"C\" { fn f(); }",
        ]);
    }

    #[test]
    fn strings_and_comments_do_not_trigger() {
        expect_clean(&[
            "// #[allow(unsafe_code)]\nfn f() {}",
            "/* #[allow(unsafe_code)] */\nfn f() {}",
            "/* outer /* #[allow(unsafe_code)] */ still comment */\nfn f() {}",
            "let s = \"#[allow(unsafe_code)]\";",
            "let s = \"line one\n#[allow(unsafe_code)]\nline three\";",
            "let s = r#\"#[allow(unsafe_code)]\"#;",
            "let s = b\"#[allow(unsafe_code)]\";",
            "#[doc = \"allow(unsafe_code)\"]\nfn f() {}",
        ]);
    }

    #[test]
    fn lifetimes_and_chars_do_not_confuse_tokenizer() {
        expect_clean(&[
            "fn f<'a>(x: &'a str) -> char { let c = 'a'; let q = '\\''; x.as_bytes()[0] as char }\n",
            "fn f() { let q = b'\"'; let bs = b'\\\\'; let h = '\\x41'; let u = '\\u{1F600}'; }",
        ]);
    }

    #[test]
    fn raw_ident_levels_found() {
        expect("#[r#allow(unsafe_code)]", &["outer:allow"]);
    }

    #[test]
    fn unbalanced_attribute_is_fail_closed() {
        for source in [
            "#[allow(unsafe_code)",
            "let s = \"unterminated;",
            "/* unterminated",
        ] {
            assert!(scan_source_text(source).is_err(), "{source}");
        }
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::{check_member_manifest, check_root_manifest, parse_manifest};

    fn parse(text: &str) -> Vec<super::Section> {
        parse_manifest(text, "fixture").expect("fixture must parse")
    }

    fn root_errors(text: &str) -> (Vec<String>, Vec<String>) {
        let sections = parse(text);
        let mut errors = Vec::new();
        let members = check_root_manifest(&sections, "fixture", &mut errors);
        (members, errors)
    }

    fn member_errors(name: &str, text: &str) -> Vec<String> {
        let sections = parse(text);
        let mut errors = Vec::new();
        check_member_manifest(name, &sections, "fixture", &mut errors);
        errors
    }

    const ROOT_OK: &str = "[workspace]\nmembers = [\"crates/a\"]\nresolver = \"2\"\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n";
    const MEMBER_OK: &str = "[package]\nname = \"tiler-core\"\n\n[lints]\nworkspace = true\n";

    fn member_ok(name: &str, text: &str) {
        let errors = member_errors(name, text);
        assert!(errors.is_empty(), "{errors:?}");
    }

    fn member_bad(name: &str, text: &str) {
        assert!(!member_errors(name, text).is_empty(), "{text}");
    }

    #[test]
    fn root_forbid_ok() {
        let (members, errors) = root_errors(ROOT_OK);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(members, vec!["crates/a"]);
    }

    #[test]
    fn root_missing_forbid_rejected() {
        let (_, errors) = root_errors("[workspace]\nmembers = [\"crates/a\"]\n");
        assert!(!errors.is_empty());
    }

    #[test]
    fn root_weakened_value_rejected() {
        let text = ROOT_OK.replace("= \"forbid\"", "= \"warn\"");
        let (_, errors) = root_errors(&text);
        assert!(!errors.is_empty());
    }

    #[test]
    fn root_unexpected_lint_table_rejected() {
        let text = format!("{ROOT_OK}\n[workspace.lints.clippy]\nall = \"deny\"\n");
        let (_, errors) = root_errors(&text);
        assert!(!errors.is_empty());
    }

    #[test]
    fn root_multiline_members_ok() {
        let text = "[workspace]\nmembers = [\n  \"crates/a\",\n  \"crates/b\", # trailing comma\n]\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n";
        let (members, errors) = root_errors(text);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(members, vec!["crates/a", "crates/b"]);
    }

    #[test]
    fn root_malformed_members_rejected() {
        // Unquoted paths are not valid TOML string arrays; the parser itself
        // must refuse them rather than checking an empty member list.
        let result = parse_manifest("[workspace]\nmembers = [crates/a]\n", "fixture");
        assert!(result.is_err());
    }

    #[test]
    fn inherit_ok() {
        member_ok("tiler-core", MEMBER_OK);
    }

    #[test]
    fn inherit_with_extra_key_rejected() {
        member_bad(
            "tiler-core",
            "[package]\nname = \"tiler-core\"\n\n[lints]\nworkspace = true\nunknown = true\n",
        );
    }

    #[test]
    fn inherit_plus_override_rejected() {
        // Cargo cannot inherit workspace lints and override one key: a crate
        // trying both must fail instead of silently picking one.
        member_bad(
            "tiler-core",
            "[package]\nname = \"tiler-core\"\n\n[lints]\nworkspace = true\n\n[lints.rust]\nunsafe_code = \"allow\"\n",
        );
    }

    #[test]
    fn deny_exception_ok_for_registered_crates() {
        for name in ["tiler-windows", "tiler-macos", "tiler-kwin-effect-ffi"] {
            member_ok(
                name,
                &format!("[package]\nname = \"{name}\"\n\n[lints.rust]\nunsafe_code = \"deny\"\n"),
            );
        }
    }

    #[test]
    fn deny_exception_rejected_for_other_crates() {
        member_bad(
            "tiler-core",
            "[package]\nname = \"tiler-core\"\n\n[lints.rust]\nunsafe_code = \"deny\"\n",
        );
    }

    #[test]
    fn deny_exception_wrong_value_rejected() {
        member_bad(
            "tiler-windows",
            "[package]\nname = \"tiler-windows\"\n\n[lints.rust]\nunsafe_code = \"warn\"\n",
        );
    }

    #[test]
    fn no_lints_rejected_for_any_crate() {
        // No crate-wide bypass exists: a member without a lint posture fails,
        // whether brand-new or previously unconfigured.
        for name in ["tiler-new", "tiler-kwin-effect-ffi"] {
            member_bad(name, &format!("[package]\nname = \"{name}\"\n"));
        }
    }

    #[test]
    fn unknown_lint_group_rejected() {
        member_bad(
            "tiler-core",
            "[package]\nname = \"tiler-core\"\n\n[lints]\nworkspace = true\n\n[lints.clippy]\nall = \"deny\"\n",
        );
    }

    #[test]
    fn member_workspace_table_rejected() {
        member_bad(
            "tiler-core",
            "[package]\nname = \"tiler-core\"\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n",
        );
    }

    #[test]
    fn triple_quoted_manifest_rejected() {
        let result = parse_manifest("[package]\nname = \"\"\"x\"\"\"\n", "fixture");
        assert!(result.is_err());
    }
}

// End-to-end: a miniature workspace proving manifest, scan, allowlist, and discovery interplay.
#[cfg(test)]
mod e2e_tests {
    use super::check_workspace;
    use std::fs;

    fn e2e_tree(tag: &str, extra: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omnitiler-guardrail-e2e-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut files: Vec<(&str, &str)> = vec![
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/ok\", \"crates/tiler-windows\", \"crates/tiler-macos\", \"crates/tiler-kwin-effect-ffi\", \"crates/tiler-live-fixture\"]\nresolver = \"2\"\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n",
            ),
            (
                "crates/ok/Cargo.toml",
                "[package]\nname = \"ok\"\n\n[lints]\nworkspace = true\n",
            ),
            ("crates/ok/src/lib.rs", "#![forbid(unsafe_code)]\n"),
            (
                "crates/tiler-windows/Cargo.toml",
                "[package]\nname = \"tiler-windows\"\n\n[lints.rust]\nunsafe_code = \"deny\"\n",
            ),
            (
                "crates/tiler-windows/src/lib.rs",
                "#![deny(unsafe_code)]\n#[cfg(windows)]\npub mod ffi;\n",
            ),
            (
                "crates/tiler-windows/src/ffi/mod.rs",
                "#![allow(unsafe_code)]\n",
            ),
            (
                "crates/tiler-macos/Cargo.toml",
                "[package]\nname = \"tiler-macos\"\n\n[lints.rust]\nunsafe_code = \"deny\"\n",
            ),
            (
                "crates/tiler-macos/src/lib.rs",
                "#![deny(unsafe_code)]\npub mod ffi;\n",
            ),
            ("crates/tiler-macos/src/ffi.rs", "#![allow(unsafe_code)]\n"),
            (
                "crates/tiler-kwin-effect-ffi/Cargo.toml",
                "[package]\nname = \"tiler-kwin-effect-ffi\"\n\n[lints.rust]\nunsafe_code = \"deny\"\n",
            ),
            (
                "crates/tiler-kwin-effect-ffi/src/lib.rs",
                "#![deny(unsafe_code)]\npub mod drag_oracle;\n",
            ),
            (
                "crates/tiler-kwin-effect-ffi/src/drag_oracle.rs",
                "#![allow(unsafe_code)]\n",
            ),
            (
                "crates/tiler-live-fixture/Cargo.toml",
                "[package]\nname = \"tiler-live-fixture\"\n\n[lints.rust]\nunsafe_code = \"deny\"\n",
            ),
            (
                "crates/tiler-live-fixture/src/pipe_win.rs",
                "#![allow(unsafe_code)]\n",
            ),
        ];
        files.extend_from_slice(extra);
        for (rel, content) in files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            fs::write(&path, content).expect("write");
        }
        root
    }

    #[test]
    fn approved_tree_passes() {
        let root = e2e_tree("ok", &[]);
        let errors = check_workspace(&root);
        let _ = fs::remove_dir_all(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn stray_allow_and_undeclared_crate_fail() {
        let root = e2e_tree(
            "bad",
            &[
                ("crates/ok/src/stray.rs", "#[allow(unsafe_code)]\n"),
                (
                    "crates/sneaky/Cargo.toml",
                    "[package]\nname = \"tiler-sneaky\"\n",
                ),
            ],
        );
        let errors = check_workspace(&root);
        let _ = fs::remove_dir_all(&root);
        assert!(
            errors.iter().any(|e| e.contains("stray.rs")),
            "stray allow missed: {errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("sneaky")),
            "undeclared crate missed: {errors:?}"
        );
    }
}
