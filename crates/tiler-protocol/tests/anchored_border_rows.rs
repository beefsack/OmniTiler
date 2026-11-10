//! Staged v4 anchored-border rows (unit3): strict shape-only row codec for
//! [`BorderPresentationV4`](tiler_core::world_report::BorderPresentationV4)
//! and [`BorderHideReasonV4`](tiler_core::world_report::BorderHideReasonV4).
//! Offline only: no adapters, no live work, no engine.
//!
//! Fixture rule: every `test-fixtures/world-wire/v4/messages/*.json`
//! WorldReply valid-wire file is an immutable standalone shape witness.
//! Tests extract the canonical `presentation.border` fragment BYTES from
//! the full message (preserving original key order, never serde-sorted),
//! run exact row decode plus checked canonical encode, and compare BYTES,
//! not merely `Value` equality. Nothing here claims whole-reply
//! acceptance or producer semantics: mixed descriptor states are
//! shape-accepted, and the full v4 reply route is untouched.

use std::collections::BTreeSet;

use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::world_report::{BorderHideReasonV4, BorderPresentationV4, FrameWitness};
use tiler_core::world_sizing::anchored_border_estimate;
use tiler_protocol::world_protocol::{
    WORLD_MAX_REQUEST_BYTES, decode_border_presentation_v4, decode_border_presentation_v4_str,
    encode_border_presentation_v4,
};

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4")
}

// --- Byte-exact fragment extraction ------------------------------------------
// Locate `"key":` then scan the raw value with string/escape-aware bracket
// depth. No JSON re-serialization, so original key order is preserved.

fn value_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    match bytes[start] {
        b'"' => {
            let mut i = start + 1;
            while i < text.len() {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'"' => return i + 1,
                    _ => i += 1,
                }
            }
            panic!("unterminated string at {start}");
        }
        b'{' | b'[' => {
            let (open, close) = if bytes[start] == b'{' {
                (b'{', b'}')
            } else {
                (b'[', b']')
            };
            let mut depth = 0usize;
            let mut i = start;
            let mut in_str = false;
            let mut esc = false;
            while i < text.len() {
                let c = bytes[i];
                if in_str {
                    if esc {
                        esc = false;
                    } else if c == b'\\' {
                        esc = true;
                    } else if c == b'"' {
                        in_str = false;
                    }
                } else if c == b'"' {
                    in_str = true;
                } else if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                i += 1;
            }
            panic!("unterminated composite at {start}");
        }
        b'n' => start + 4,
        b't' => start + 4,
        b'f' => start + 5,
        b'-' | b'0'..=b'9' => {
            let mut i = start;
            while i < text.len()
                && !matches!(bytes[i], b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t')
            {
                i += 1;
            }
            i
        }
        c => panic!("unexpected value start {c} at {start}"),
    }
}

/// Extract the raw value bytes carried for `key` (first occurrence in this
/// scope; callers scope nested keys through their parent fragment).
fn extract_key<'a>(text: &'a str, key: &str) -> &'a str {
    let pat = format!("\"{key}\"");
    let rel = text
        .find(&pat)
        .unwrap_or_else(|| panic!("key {key} present"));
    let mut i = rel + pat.len();
    while text.as_bytes()[i].is_ascii_whitespace() {
        i += 1;
    }
    assert_eq!(text.as_bytes()[i], b':', "key {key} colon");
    i += 1;
    while text.as_bytes()[i].is_ascii_whitespace() {
        i += 1;
    }
    let end = value_end(text, i);
    &text[i..end]
}

fn read_message(root: &std::path::Path, file: &str) -> String {
    std::fs::read_to_string(root.join(file)).unwrap_or_else(|_| panic!("read {file}"))
}

fn reply_messages() -> Vec<(String, String)> {
    let root = fixture_root();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifest.json")).expect("manifest"),
    )
    .expect("manifest JSON");
    manifest["fixtures"]
        .as_array()
        .expect("fixtures array")
        .iter()
        .filter(|f| f["family"] == "WorldReply" && f["expectation"] == "valid-wire")
        .map(|f| {
            (
                f["id"].as_str().expect("id").to_owned(),
                f["file"].as_str().expect("file").to_owned(),
            )
        })
        .collect()
}

// --- Fixture census -----------------------------------------------------------

#[test]
fn v4_anchored_border_reply_census() {
    // 115 valid-wire replies: every border fragment decodes, re-encodes to
    // the exact carried bytes, and sizes exactly. 101 visible rows carry
    // the empty reason with full descriptors; 14 hidden rows carry a closed
    // token with all-null descriptors. 101 witness objects, 14 nulls.
    let root = fixture_root();
    let messages = reply_messages();
    assert_eq!(messages.len(), 115, "valid-wire reply census");
    let mut visible = 0usize;
    let mut hidden = 0usize;
    let mut witness_objects = 0usize;
    let mut witness_nulls = 0usize;
    let mut reasons = BTreeSet::new();
    for (id, file) in &messages {
        let text = read_message(&root, file);
        let pres = extract_key(&text, "presentation");
        let frag = extract_key(pres, "border");
        let row = decode_border_presentation_v4_str(frag)
            .unwrap_or_else(|_| panic!("{id} border decodes"));
        let encoded = encode_border_presentation_v4(&row)
            .unwrap_or_else(|_| panic!("{id} border re-encodes"));
        assert_eq!(encoded, frag, "{id} border round-trips byte-exact");
        assert_eq!(
            anchored_border_estimate(&row),
            encoded.len(),
            "{id} border sizes exact"
        );
        if row.visible {
            visible += 1;
        } else {
            hidden += 1;
        }
        if row.witness.is_some() {
            witness_objects += 1;
        } else {
            witness_nulls += 1;
        }
        reasons.insert(row.reason.map_or(String::new(), |r| r.as_str().to_owned()));
    }
    assert_eq!(visible, 101, "visible border rows");
    assert_eq!(hidden, 14, "hidden border rows");
    assert_eq!(witness_objects, 101, "witness objects");
    assert_eq!(witness_nulls, 14, "null witnesses");
    let expected: BTreeSet<String> = [
        "",
        "disabled",
        "no-target",
        "minimized",
        "maximized",
        "fullscreen",
        "host-hidden",
        "hidden-workspace",
        "shell",
        "suspended",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(reasons, expected, "closed reason coverage");
}

#[test]
fn v4_anchored_border_reject_fixtures() {
    // The only two border reject-wire fixtures: the retired absolute
    // `inner` key refuses as unknown-field; v3 `cloaked` refuses as
    // unknown-value.
    let root = fixture_root();
    for (id, kind) in [
        ("invalid-border-old-absolute", "unknown-field"),
        ("invalid-border-cloaked", "unknown-value"),
    ] {
        let text = read_message(&root, &format!("messages/{id}.json"));
        let pres = extract_key(&text, "presentation");
        let frag = extract_key(pres, "border");
        let error = decode_border_presentation_v4_str(frag).unwrap_err();
        assert_eq!(error.kind, kind, "{id} refuses as {kind}");
    }
}

// --- Constructed shape edges ---------------------------------------------------

fn visible_json() -> serde_json::Value {
    serde_json::json!({
        "visible": true, "reason": "",
        "subject": "win-a", "outsets": [3, 3, 3, 3],
        "width": 3, "radius": 6,
        "witness": {"win": "win-a", "obs": "1",
            "frame": [8, 8, 400, 600], "foreground": "win-a"},
    })
}

fn hidden_json(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "visible": false, "reason": reason,
        "subject": null, "outsets": null,
        "width": null, "radius": null, "witness": null,
    })
}

fn assert_size_eq(row: &BorderPresentationV4) {
    let encoded = encode_border_presentation_v4(row).expect("edge encodes");
    assert_eq!(
        anchored_border_estimate(row),
        encoded.len(),
        "edge sizes exact"
    );
    let reparsed = decode_border_presentation_v4_str(&encoded).expect("edge re-decodes");
    assert_eq!(&reparsed, row, "edge round-trips");
}

#[test]
fn v4_border_reason_tokens_and_visible_coupling() {
    // All 9 closed tokens decode hidden with all-null descriptors and
    // re-encode byte-exact with exact sizes.
    for token in [
        "disabled",
        "no-target",
        "minimized",
        "maximized",
        "fullscreen",
        "host-hidden",
        "hidden-workspace",
        "shell",
        "suspended",
    ] {
        let row = decode_border_presentation_v4_str(&hidden_json(token).to_string())
            .unwrap_or_else(|_| panic!("{token} decodes"));
        assert!(!row.visible);
        assert!(row.witness.is_none());
        assert_size_eq(&row);
    }
    // Hidden with the empty reason stays valid (geometry-veto
    // carry-forward): eligible but uncomputable bounds hide with no reason.
    let veto = decode_border_presentation_v4_str(&hidden_json("").to_string())
        .expect("hidden empty reason decodes");
    assert!(!veto.visible);
    assert_eq!(veto.reason, None);
    assert_size_eq(&veto);
    // Visible with any reason token refuses as unknown-value (the contract
    // table requires empty when visible). Visible with "" decodes.
    for token in [
        "disabled",
        "no-target",
        "minimized",
        "maximized",
        "fullscreen",
        "host-hidden",
        "hidden-workspace",
        "shell",
        "suspended",
    ] {
        let mut bad = visible_json();
        bad["reason"] = serde_json::Value::String(token.to_owned());
        assert_eq!(
            decode_border_presentation_v4_str(&bad.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "visible {token} refuses"
        );
    }
    decode_border_presentation_v4_str(&visible_json().to_string()).expect("visible decodes");
    // v3 `cloaked` and unknown tokens refuse as unknown-value, never a new token.
    for bad in ["cloaked", "Cloaked", "HOST-HIDDEN", "geometry-veto", ""] {
        if bad.is_empty() {
            continue;
        }
        let mut row = hidden_json("disabled");
        row["reason"] = serde_json::Value::String(bad.to_owned());
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "reason {bad:?} refuses"
        );
    }
    // Reason with the wrong JSON type refuses as malformed; missing reason
    // refuses as malformed.
    for bad in [serde_json::json!(1), serde_json::json!(true)] {
        let mut row = hidden_json("disabled");
        row["reason"] = bad;
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "malformed",
            "reason type refuses"
        );
    }
}

#[test]
fn v4_border_descriptor_masks_are_independent() {
    // All 32 null/present masks over the five descriptor fields, on both
    // the visible-empty and hidden-token base rows: masked slots are null,
    // unmasked slots carry fixture values. Every mask decodes, checked
    // encodes, and sizes exactly. The row shape imposes no descriptor
    // coupling and does not validate causal or producer consistency.
    const FIELDS: [&str; 5] = ["subject", "outsets", "width", "radius", "witness"];
    fn present_value(key: &str) -> serde_json::Value {
        match key {
            "subject" => serde_json::json!("win-a"),
            "outsets" => serde_json::json!([3, 3, 3, 3]),
            "width" => serde_json::json!(3),
            "radius" => serde_json::json!(6),
            "witness" => serde_json::json!({"win": "win-a", "obs": "1",
                "frame": [8, 8, 400, 600], "foreground": "win-a"}),
            _ => unreachable!("descriptor field"),
        }
    }
    for mask in 0..32u32 {
        for (visible, reason) in [(true, ""), (false, "minimized")] {
            let mut row = if visible {
                visible_json()
            } else {
                hidden_json(reason)
            };
            for (bit, key) in FIELDS.iter().enumerate() {
                row[key] = if mask & (1 << bit) != 0 {
                    serde_json::Value::Null
                } else {
                    present_value(key)
                };
            }
            let parsed = decode_border_presentation_v4_str(&row.to_string())
                .unwrap_or_else(|_| panic!("mask {mask:05b} visible={visible} decodes"));
            assert_eq!(parsed.visible, visible, "mask {mask:05b} visible");
            assert_eq!(
                parsed.subject.is_none(),
                mask & 0b00001 != 0,
                "mask {mask:05b} subject"
            );
            assert_eq!(
                parsed.outsets.is_none(),
                mask & 0b00010 != 0,
                "mask {mask:05b} outsets"
            );
            assert_eq!(
                parsed.width.is_none(),
                mask & 0b00100 != 0,
                "mask {mask:05b} width"
            );
            assert_eq!(
                parsed.radius.is_none(),
                mask & 0b01000 != 0,
                "mask {mask:05b} radius"
            );
            assert_eq!(
                parsed.witness.is_none(),
                mask & 0b10000 != 0,
                "mask {mask:05b} witness"
            );
            assert_size_eq(&parsed);
        }
    }
}

#[test]
fn v4_border_dangling_ids_are_shape_only() {
    // Dangling ids are accepted here; the row shape does not validate
    // causal or producer consistency.
    let mut dangling = visible_json();
    dangling["subject"] = serde_json::json!("ghost-win");
    dangling["witness"] = serde_json::json!({"win": "ghost-other", "obs": "9",
        "frame": [0, 0, 8, 8], "foreground": "ghost-fg"});
    let row = decode_border_presentation_v4_str(&dangling.to_string()).expect("dangling decodes");
    assert_size_eq(&row);
}

#[test]
fn v4_border_missing_keys_types_and_key_closure() {
    // Missing EACH required slot refuses as malformed.
    for key in [
        "visible", "reason", "subject", "outsets", "width", "radius", "witness",
    ] {
        let mut row = visible_json();
        row.as_object_mut().expect("object").remove(key);
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "malformed",
            "missing {key} refuses"
        );
    }
    // Wrong JSON types refuse as malformed (unknown keys and bad tokens
    // keep their own kinds below).
    let mut bad = visible_json();
    bad["visible"] = serde_json::json!("true");
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "visible string refuses"
    );
    let mut bad = visible_json();
    bad["visible"] = serde_json::Value::Null;
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "visible null refuses"
    );
    let mut bad = visible_json();
    bad["subject"] = serde_json::json!(1);
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "subject number refuses"
    );
    let mut bad = visible_json();
    bad["outsets"] = serde_json::json!({"l": 3});
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "outsets object refuses"
    );
    let mut bad = visible_json();
    bad["width"] = serde_json::json!("3");
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "width string refuses"
    );
    let mut bad = visible_json();
    bad["radius"] = serde_json::json!(true);
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "radius bool refuses"
    );
    let mut bad = visible_json();
    bad["witness"] = serde_json::json!("witness");
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "witness string refuses"
    );
    let mut bad = visible_json();
    bad["witness"] = serde_json::json!([]);
    assert_eq!(
        decode_border_presentation_v4_str(&bad.to_string())
            .unwrap_err()
            .kind,
        "malformed",
        "witness array refuses"
    );
    // Unknown keys refuse as unknown-field, including the retired absolute
    // `inner`/`outer` geometry and any other extra.
    for key in ["inner", "outer", "paused", "borderEnabled"] {
        let mut row = visible_json();
        row[key] = serde_json::json!(1);
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "unknown-field",
            "extra {key} refuses"
        );
    }
}

#[test]
fn v4_border_id_slots_length_and_alphabet() {
    // Every carried id slot accepts 128 opaque bytes and refuses 129,
    // empty, and bad-alphabet spellings as unknown-value. Subject is
    // nullable, so only its present form is tested here.
    let ok_128 = "w".repeat(128);
    let bad_129 = "w".repeat(129);
    let bad_alpha = ["not an id!", "o\u{00fc}t", "a/b"];
    for slot in ["win", "foreground"] {
        let build = |candidate: &str| {
            let (win, foreground) = match slot {
                "win" => (candidate.to_owned(), "win-a".to_owned()),
                _ => ("win-a".to_owned(), candidate.to_owned()),
            };
            serde_json::json!({
                "visible": true, "reason": "",
                "subject": "win-a", "outsets": [3, 3, 3, 3],
                "width": 3, "radius": 6,
                "witness": {"win": win, "obs": "1",
                    "frame": [0, 0, 8, 8], "foreground": foreground},
            })
            .to_string()
        };
        decode_border_presentation_v4_str(&build(&ok_128))
            .unwrap_or_else(|_| panic!("witness {slot} 128 decodes"));
        let mut bads = vec![bad_129.clone(), String::new()];
        bads.extend(bad_alpha.iter().map(|s| s.to_string()));
        for bad in &bads {
            assert_eq!(
                decode_border_presentation_v4_str(&build(bad))
                    .unwrap_err()
                    .kind,
                "unknown-value",
                "witness {slot} {bad:?} refuses"
            );
        }
    }
    let mut subject = visible_json();
    subject["subject"] = serde_json::Value::String(ok_128.clone());
    decode_border_presentation_v4_str(&subject.to_string()).expect("subject 128 decodes");
    let mut bads = vec![bad_129.clone(), String::new()];
    bads.extend(bad_alpha.iter().map(|s| s.to_string()));
    for bad in &bads {
        let mut row = visible_json();
        row["subject"] = serde_json::Value::String(bad.clone());
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "subject {bad:?} refuses"
        );
    }
}

#[test]
fn v4_border_counters_rects_styles_and_outsets() {
    // Witness counters: 0/9/10/u64::MAX canonical; JSON numbers, leading
    // zeros, signs, empties, and overflow refuse.
    for good in ["0", "9", "10", "18446744073709551615"] {
        let mut row = visible_json();
        row["witness"] = serde_json::json!({"win": "win-a", "obs": good,
            "frame": [0, 0, 8, 8], "foreground": "win-a"});
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .expect("counter decodes")
                .witness
                .expect("witness")
                .obs,
            good.parse::<u64>().expect("counter"),
            "obs {good}"
        );
    }
    for bad in [
        serde_json::json!({"win": "win-a", "obs": 1, "frame": [0, 0, 8, 8], "foreground": "win-a"}),
        serde_json::json!({"win": "win-a", "obs": "007", "frame": [0, 0, 8, 8], "foreground": "win-a"}),
        serde_json::json!({"win": "win-a", "obs": "-1", "frame": [0, 0, 8, 8], "foreground": "win-a"}),
        serde_json::json!({"win": "win-a", "obs": "", "frame": [0, 0, 8, 8], "foreground": "win-a"}),
        serde_json::json!({"win": "win-a", "obs": "18446744073709551616", "frame": [0, 0, 8, 8], "foreground": "win-a"}),
    ] {
        let mut row = visible_json();
        row["witness"] = bad;
        assert!(
            decode_border_presentation_v4_str(&row.to_string()).is_err(),
            "obs refuses"
        );
    }
    // Witness frames share the carried-rect rule: zero/negative extents
    // and out-of-bound coords refuse as unknown-value; wrong arity and
    // non-integer types refuse as malformed.
    for bad_frame in [
        serde_json::json!([0, 0, 0, 8]),
        serde_json::json!([0, 0, -1, 8]),
        serde_json::json!([16385, 0, 8, 8]),
        serde_json::json!([0, 0, 16385, 8]),
    ] {
        let mut row = visible_json();
        row["witness"] = serde_json::json!({"win": "win-a", "obs": "1",
            "frame": bad_frame, "foreground": "win-a"});
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "frame {bad_frame} refuses"
        );
    }
    for bad_frame in [
        serde_json::json!([0, 0, 8]),
        serde_json::json!([0, 0, 8, 8, 1]),
        serde_json::json!("frame"),
        serde_json::json!([0.5, 0, 8, 8]),
    ] {
        let mut row = visible_json();
        row["witness"] = serde_json::json!({"win": "win-a", "obs": "1",
            "frame": bad_frame, "foreground": "win-a"});
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "malformed",
            "frame {bad_frame} refuses"
        );
    }
    // Outsets: exactly four bare JSON integers in i32 range (any sign at
    // shape level; no Rect bound applies to thickness rows). Signed
    // extrema are accepted with exact sizes.
    let mut extrema = visible_json();
    extrema["outsets"] = serde_json::json!([-2147483648, -1, 0, 2147483647]);
    let row =
        decode_border_presentation_v4_str(&extrema.to_string()).expect("outset extrema decode");
    assert_eq!(
        row.outsets,
        Some([-2147483648, -1, 0, 2147483647]),
        "outset extrema"
    );
    assert_size_eq(&row);
    for bad_outsets in [
        serde_json::json!([3, 3, 3]),
        serde_json::json!([3, 3, 3, 3, 3]),
        serde_json::json!("outsets"),
        serde_json::json!([3.5, 3, 3, 3]),
        serde_json::json!(["3", 3, 3, 3]),
        serde_json::json!([2147483648_i64, 3, 3, 3]),
        serde_json::json!([-2147483649_i64, 3, 3, 3]),
    ] {
        let mut row = visible_json();
        row["outsets"] = bad_outsets;
        assert_eq!(
            decode_border_presentation_v4_str(&row.to_string())
                .unwrap_err()
                .kind,
            "malformed",
            "outsets shape refuses"
        );
    }
    // Width/radius: 0 and i32::MAX accepted with exact sizes; negatives
    // refuse as unknown-value; floats, strings, and i32 overflow refuse as
    // malformed.
    for good in [0, 1, 2147483647] {
        for key in ["width", "radius"] {
            let mut row = visible_json();
            row[key] = serde_json::json!(good);
            let parsed = decode_border_presentation_v4_str(&row.to_string())
                .unwrap_or_else(|_| panic!("{key} {good} decodes"));
            assert_size_eq(&parsed);
        }
    }
    for key in ["width", "radius"] {
        for bad in [-1, -2147483648] {
            let mut row = visible_json();
            row[key] = serde_json::json!(bad);
            assert_eq!(
                decode_border_presentation_v4_str(&row.to_string())
                    .unwrap_err()
                    .kind,
                "unknown-value",
                "{key} {bad} refuses"
            );
        }
        for bad in [
            serde_json::json!(3.5),
            serde_json::json!("3"),
            serde_json::json!(true),
            serde_json::json!(2147483648_i64),
        ] {
            let mut row = visible_json();
            row[key] = bad;
            assert_eq!(
                decode_border_presentation_v4_str(&row.to_string())
                    .unwrap_err()
                    .kind,
                "malformed",
                "{key} type refuses"
            );
        }
    }
}

#[test]
fn v4_border_maximum_edge_sizes_exact() {
    // Maximum valid edge: 128-byte ids, u64::MAX obs, extreme rect and
    // outset integers, i32::MAX styles. Sizes stay exact at the boundary.
    let win_128 = "w".repeat(128);
    let row = BorderPresentationV4 {
        visible: true,
        reason: None,
        subject: Some(WindowId(win_128.clone())),
        outsets: Some([i32::MIN, -1, 0, i32::MAX]),
        width: Some(i32::MAX),
        radius: Some(i32::MAX),
        witness: Some(FrameWitness {
            win: WindowId(win_128.clone()),
            obs: u64::MAX,
            frame: Rect {
                x: -16384,
                y: -16384,
                w: 16384,
                h: 16384,
            },
            foreground: WindowId(win_128.clone()),
        }),
    };
    assert_size_eq(&row);
    // Maximum hidden edge with empty reason (geometry veto) also sizes exact.
    let hidden = BorderPresentationV4 {
        visible: false,
        reason: None,
        subject: None,
        outsets: None,
        width: None,
        radius: None,
        witness: None,
    };
    assert_size_eq(&hidden);
    // Every closed token at maximum ids sizes exact.
    for reason in [
        BorderHideReasonV4::Disabled,
        BorderHideReasonV4::NoTarget,
        BorderHideReasonV4::Minimized,
        BorderHideReasonV4::Maximized,
        BorderHideReasonV4::Fullscreen,
        BorderHideReasonV4::HostHidden,
        BorderHideReasonV4::HiddenWorkspace,
        BorderHideReasonV4::Shell,
        BorderHideReasonV4::Suspended,
    ] {
        let row = BorderPresentationV4 {
            visible: false,
            reason: Some(reason),
            subject: Some(WindowId(win_128.clone())),
            outsets: Some([i32::MAX; 4]),
            width: Some(0),
            radius: Some(0),
            witness: Some(FrameWitness {
                win: WindowId(win_128.clone()),
                obs: 0,
                frame: Rect {
                    x: 0,
                    y: 0,
                    w: 8,
                    h: 8,
                },
                foreground: WindowId(win_128.clone()),
            }),
        };
        assert_size_eq(&row);
    }
}

#[test]
fn v4_border_checked_encoders_reject_invalid_typed_rows() {
    // Encoders never emit unsafe JSON: invalid typed ids, the
    // visible/reason coupling, negative styles, and bad geometry return
    // Err instead of encoding.
    let rect = Rect {
        x: 0,
        y: 0,
        w: 8,
        h: 8,
    };
    let witness = |win: &str, foreground: &str| FrameWitness {
        win: WindowId(win.to_owned()),
        obs: 1,
        frame: rect,
        foreground: WindowId(foreground.to_owned()),
    };
    let base = || BorderPresentationV4 {
        visible: true,
        reason: None,
        subject: Some(WindowId("win-a".to_owned())),
        outsets: Some([3, 3, 3, 3]),
        width: Some(3),
        radius: Some(6),
        witness: Some(witness("win-a", "win-a")),
    };
    let mut bad = base();
    bad.subject = Some(WindowId(String::new()));
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.subject = Some(WindowId("not an id!".to_owned()));
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.reason = Some(BorderHideReasonV4::Minimized);
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.width = Some(-1);
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.radius = Some(-1);
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.witness = Some(witness("", "win-a"));
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.witness = Some(witness("win-a", ""));
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.witness = Some(witness("win-a", "not an id!"));
    assert!(encode_border_presentation_v4(&bad).is_err());
    let mut bad = base();
    bad.witness = Some(FrameWitness {
        win: WindowId("win-a".to_owned()),
        obs: 1,
        frame: Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 8,
        },
        foreground: WindowId("win-a".to_owned()),
    });
    assert!(encode_border_presentation_v4(&bad).is_err());
    // The Value decoder agrees with the string entry point on one valid row.
    let text = encode_border_presentation_v4(&base()).expect("base encodes");
    let value: serde_json::Value = serde_json::from_str(&text).expect("fragment");
    assert_eq!(
        decode_border_presentation_v4(&value).expect("value decodes"),
        base()
    );
}

#[test]
fn v4_border_duplicate_keys_and_size_gate() {
    // Duplicate keys refuse, including escaped-spelling duplicates.
    let dup = r#"{"visible":true,"visible":false,"reason":"","subject":null,"outsets":null,"width":null,"radius":null,"witness":null}"#;
    assert_eq!(
        decode_border_presentation_v4_str(dup).unwrap_err().kind,
        "malformed",
        "duplicate key refuses"
    );
    let escaped = "{\"visible\":true,\"\\u0076isible\":false,\"reason\":\"\",\"subject\":null,\"outsets\":null,\"width\":null,\"radius\":null,\"witness\":null}";
    assert_eq!(
        decode_border_presentation_v4_str(escaped).unwrap_err().kind,
        "malformed",
        "escaped duplicate refuses"
    );
    // Nested duplicates refuse at the string entry point too, including
    // escaped spellings inside the witness object. Raw literals preserve
    // the literal JSON backslash-u escape.
    let nested = r#"{"visible":true,"reason":"","subject":"win-a","outsets":[3,3,3,3],"width":3,"radius":6,"witness":{"win":"win-a","obs":"1","frame":[0,0,8,8],"foreground":"win-a","foreground":"win-b"}}"#;
    assert_eq!(
        decode_border_presentation_v4_str(nested).unwrap_err().kind,
        "malformed",
        "nested duplicate refuses"
    );
    let nested_escaped = r#"{"visible":true,"reason":"","subject":"win-a","outsets":[3,3,3,3],"width":3,"radius":6,"witness":{"win":"win-a","obs":"1","frame":[0,0,8,8],"foreground":"win-a","\u0066oreground":"win-b"}}"#;
    assert_eq!(
        decode_border_presentation_v4_str(nested_escaped)
            .unwrap_err()
            .kind,
        "malformed",
        "nested escaped duplicate refuses"
    );
    // Row input bound mirrors the current route: oversized refuses.
    let big = "x".repeat(WORLD_MAX_REQUEST_BYTES + 1);
    assert_eq!(
        decode_border_presentation_v4_str(&big).unwrap_err().kind,
        "oversized"
    );
}
