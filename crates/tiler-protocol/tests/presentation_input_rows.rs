//! Staged v4 presentation input rows (unit2): strict shape-only row codecs
//! for [`PresentationIngressV4`](tiler_core::world_report::PresentationIngressV4),
//! [`FrameWitness`](tiler_core::world_report::FrameWitness),
//! [`RendererInput`](tiler_core::world_report::RendererInput), and the v4 pager
//! row. Offline only: no D-Bus, no KWin, no engine.
//!
//! Fixture rule: every `test-fixtures/world-wire/v4/messages/*.json` file is
//! an immutable standalone shape witness. Tests extract canonical fragment
//! BYTES from the full message (preserving original key order, never
//! serde-sorted), run exact row decode plus checked canonical encode, and
//! compare BYTES, not merely `Value` equality. Nothing here claims whole v4
//! report acceptance: full v4 envelopes still refuse at the unchanged v3
//! whole-report gate (`unknown-field` on the staged `rendererInput` key
//! before any version verdict), which is locked below.

use std::collections::BTreeSet;

use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::world_report::{
    FrameWitness, PresentationIngressV4, PresentationPagerFrameV4, RenderTarget, RendererInput,
};
use tiler_protocol::world_protocol::{
    WORLD_MAX_REQUEST_BYTES, decode_frame_witness_str, decode_pager_frame_v4_str,
    decode_presentation_ingress_v4, decode_presentation_ingress_v4_str, decode_renderer_input_str,
    decode_request_legacy, decode_request_with_wire_legacy, encode_frame_witness,
    encode_pager_frame_v4, encode_presentation_ingress_v4, encode_render_target,
    encode_renderer_input,
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

/// Split a raw `[...]` array into its raw element bytes.
fn array_elements(array: &str) -> Vec<&str> {
    let bytes = array.as_bytes();
    assert_eq!(bytes[0], b'[', "array open");
    let mut out = Vec::new();
    let mut i = 1;
    loop {
        while i < array.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= array.len() || bytes[i] == b']' {
            break;
        }
        let end = value_end(array, i);
        out.push(&array[i..end]);
        i = end;
    }
    out
}

fn read_message(root: &std::path::Path, file: &str) -> String {
    std::fs::read_to_string(root.join(file)).unwrap_or_else(|_| panic!("read {file}"))
}

fn report_messages() -> Vec<(String, String, String)> {
    let root = fixture_root();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifest.json")).expect("manifest"),
    )
    .expect("manifest JSON");
    manifest["fixtures"]
        .as_array()
        .expect("fixtures array")
        .iter()
        .filter(|f| f["family"] == "WorldReport")
        .filter(|f| f["expectation"] == "valid-wire" || f["expectation"] == "accepted-held-row")
        .map(|f| {
            (
                f["id"].as_str().expect("id").to_owned(),
                f["file"].as_str().expect("file").to_owned(),
                f["expectation"].as_str().expect("expectation").to_owned(),
            )
        })
        .collect()
}

// --- Fixture census -----------------------------------------------------------

#[test]
fn v4_presentation_ingress_fixture_census() {
    // 207 reports (126 valid + 81 held): 196 strict-valid ingress rows
    // (186 with a witness object, 10 with null) plus 11 expected row
    // refusals (8 top-level removed keys, 2 pager float/tile rows, 1 witness
    // constant string). Exact row decode plus checked canonical encode must
    // reproduce the carried fragment BYTES.
    let root = fixture_root();
    let messages = report_messages();
    assert_eq!(messages.len(), 207, "positive + held report census");
    let expected_refusals: BTreeSet<&str> = [
        "invalid-removed-presentation-borderEnabled",
        "invalid-removed-presentation-underlayEnabled",
        "invalid-removed-presentation-borderGap",
        "invalid-removed-presentation-borderWidth",
        "invalid-removed-presentation-underlayExtension",
        "invalid-removed-presentation-paused",
        "invalid-removed-presentation-cloaked",
        "invalid-removed-ingress-shell",
        "invalid-removed-pager-frame-float",
        "invalid-removed-pager-frame-tile",
        "invalid-witness-constant-string",
    ]
    .into_iter()
    .collect();
    let mut ok = 0usize;
    let mut refused: Vec<(String, &'static str)> = Vec::new();
    let mut witness_objects = 0usize;
    let mut witness_nulls = 0usize;
    for (id, file, _expectation) in &messages {
        let text = read_message(&root, file);
        let frag = extract_key(&text, "presentation");
        match decode_presentation_ingress_v4_str(frag) {
            Ok(ingress) => {
                let encoded = encode_presentation_ingress_v4(&ingress)
                    .unwrap_or_else(|_| panic!("{id} re-encodes"));
                assert_eq!(encoded, frag, "{id} ingress round-trips byte-exact");
                ok += 1;
                if ingress.witness.is_some() {
                    witness_objects += 1;
                } else {
                    witness_nulls += 1;
                }
            }
            Err(error) => refused.push((id.clone(), error.kind)),
        }
    }
    assert_eq!(ok, 196, "strict-valid ingress rows");
    assert_eq!(
        witness_objects, 186,
        "valid ingress rows carry a witness object"
    );
    assert_eq!(witness_nulls, 10, "valid ingress rows carry null witness");
    assert_eq!(refused.len(), 11, "expected row refusals");
    let refused_ids: BTreeSet<&str> = refused.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(refused_ids, expected_refusals, "refusal census");
    // Exact per-id refusal oracle: removed keys (top-level and pager
    // float/tile) refuse as unknown-field; only the constant-string
    // witness refuses as malformed (wrong JSON type).
    for (id, kind) in &refused {
        let want = if id == "invalid-witness-constant-string" {
            "malformed"
        } else {
            "unknown-field"
        };
        assert_eq!(*kind, want, "{id} refusal kind");
    }
}

#[test]
fn v4_frame_witness_objects_are_shape_valid_even_when_ingress_is_not() {
    // All 196 standalone witness objects decode as shape only, including the
    // 10 carried inside otherwise-bad presentation rows. Dangling or
    // mismatched ids are accepted here; causal join belongs to a later unit.
    let root = fixture_root();
    let messages = report_messages();
    let mut objects = 0usize;
    let mut nulls = 0usize;
    let mut bad = 0usize;
    for (id, file, _expectation) in &messages {
        let text = read_message(&root, file);
        let pres = extract_key(&text, "presentation");
        let frag = extract_key(pres, "witness");
        if frag == "null" {
            nulls += 1;
            continue;
        }
        match decode_frame_witness_str(frag) {
            Ok(witness) => {
                let encoded =
                    encode_frame_witness(&witness).unwrap_or_else(|_| panic!("{id} re-encodes"));
                assert_eq!(encoded, frag, "{id} witness round-trips byte-exact");
                objects += 1;
            }
            Err(error) => {
                assert_eq!(id, "invalid-witness-constant-string");
                assert_eq!(error.kind, "malformed");
                bad += 1;
            }
        }
    }
    assert_eq!(objects, 196, "standalone witness objects");
    assert_eq!(nulls, 10, "null witnesses");
    assert_eq!(bad, 1, "constant-string witness refusal");
}

#[test]
fn v4_pager_frame_rows_fixture_census() {
    // 197 carried pager rows: 195 valid `{win, live}` rows plus the 2
    // removed float/tile rows, which refuse as unknown-field.
    let root = fixture_root();
    let messages = report_messages();
    let mut total = 0usize;
    let mut ok = 0usize;
    let mut refused: Vec<(String, &'static str)> = Vec::new();
    for (id, file, _expectation) in &messages {
        let text = read_message(&root, file);
        let pres = extract_key(&text, "presentation");
        let frames = extract_key(pres, "pagerFrames");
        for row in array_elements(frames) {
            total += 1;
            match decode_pager_frame_v4_str(row) {
                Ok(parsed) => {
                    let encoded = encode_pager_frame_v4(&parsed)
                        .unwrap_or_else(|_| panic!("{id} pager row re-encodes"));
                    assert_eq!(encoded, row, "{id} pager row round-trips byte-exact");
                    ok += 1;
                }
                Err(error) => refused.push((id.clone(), error.kind)),
            }
        }
    }
    assert_eq!(total, 197, "carried pager rows");
    assert_eq!(ok, 195, "valid pager rows");
    assert_eq!(refused.len(), 2, "removed float/tile refusals");
    assert!(
        refused.iter().all(|(_, kind)| *kind == "unknown-field"),
        "pager removals refuse as unknown-field: {refused:?}"
    );
    let ids: BTreeSet<&str> = refused.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        BTreeSet::from([
            "invalid-removed-pager-frame-float",
            "invalid-removed-pager-frame-tile"
        ]),
        "pager refusal census"
    );
}

#[test]
fn v4_renderer_rows_shape_census_and_pairing_exclusion() {
    // 3 valid renderer arms round-trip byte-exact; 3 malformed targets
    // refuse with distinct errors (missing `win` is malformed, extra `win`
    // is unknown-field, unknown `kind` is unknown-value).
    // `invalid-renderer-without-command` is shape-VALID here: the missing
    // command pairing belongs to a later whole-report unit, not this shape
    // codec.
    let root = fixture_root();
    for (id, kind, out, ws, extra) in [
        (
            "report-renderer-workspace",
            "workspace",
            "out-a",
            "ws-a",
            None,
        ),
        (
            "report-renderer-window",
            "window",
            "out-a",
            "ws-a",
            Some(("win", "win-a")),
        ),
        (
            "report-renderer-group",
            "group",
            "out-a",
            "ws-a",
            Some(("group", "group-a")),
        ),
    ] {
        let text = read_message(&root, &format!("messages/{id}.json"));
        let frag = extract_key(&text, "rendererInput");
        let input = decode_renderer_input_str(frag).unwrap_or_else(|_| panic!("{id} decodes"));
        let encoded = encode_renderer_input(&input).unwrap_or_else(|_| panic!("{id} re-encodes"));
        assert_eq!(encoded, frag, "{id} renderer round-trips byte-exact");
        match (&input.target, extra) {
            (RenderTarget::Workspace { out: o, ws: w }, None) => {
                assert_eq!((o.0.as_str(), w.0.as_str()), (out, ws));
                assert_eq!(kind, "workspace");
            }
            (RenderTarget::Window { win, .. }, Some(("win", win_id))) => {
                assert_eq!(win.0, win_id);
            }
            (RenderTarget::Group { group, .. }, Some(("group", group_id))) => {
                assert_eq!(group.0, group_id);
            }
            (target, _) => panic!("{id} unexpected target {target:?}"),
        }
    }
    for (id, kind) in [
        ("invalid-renderer-target-missing-win", "malformed"),
        ("invalid-renderer-extra-win", "unknown-field"),
        ("invalid-renderer-unknown-kind", "unknown-value"),
    ] {
        let text = read_message(&root, &format!("messages/{id}.json"));
        let frag = extract_key(&text, "rendererInput");
        let error = decode_renderer_input_str(frag).unwrap_err();
        assert_eq!(error.kind, kind, "{id} refuses as {kind}");
    }
    let text = read_message(&root, "messages/invalid-renderer-without-command.json");
    let frag = extract_key(&text, "rendererInput");
    let input = decode_renderer_input_str(frag).expect("pairing failure is not a shape failure");
    assert!(matches!(input.target, RenderTarget::Window { .. }));
}

#[test]
fn v4_whole_report_still_refuses_before_any_version_verdict() {
    // The staged `rendererInput` key is unknown to the unchanged v3
    // whole-report gate, which runs before the version check: lock the
    // actual `unknown-field` refusal, not an `unsupported-version`
    // assumption. No whole-message conformance is claimed here.
    let root = fixture_root();
    for id in ["report-empty", "report-renderer-workspace"] {
        let text = read_message(&root, &format!("messages/{id}.json"));
        let error = decode_request_legacy(&text).expect_err("{id} whole report refuses");
        assert_eq!(
            error.kind, "unknown-field",
            "{id} refuses before version gate"
        );
        let wired = decode_request_with_wire_legacy(&text).expect_err("{id} wired refuses");
        assert_eq!(wired.kind, "unknown-field");
    }
}

// --- Constructed shape edges --------------------------------------------------

fn minimal_ingress_json() -> serde_json::Value {
    serde_json::json!({
        "foreground": "",
        "minimized": null,
        "maximized": null,
        "fullscreen": null,
        "hostHidden": null,
        "hiddenWorkspace": null,
        "frame": null,
        "chordHeld": false,
        "moveWindow": "",
        "pagerOrder": [],
        "pagerFrames": [],
        "witness": null,
        "foregroundOut": null,
    })
}

#[test]
fn v4_constructed_nulls_and_empty_tokens_round_trip() {
    // All-nullable ingress (null facts, null frame/witness/out) decodes and
    // re-encodes byte-exact; explicit empty foreground/move tokens stay
    // allowed while empty witness ids refuse.
    let value = minimal_ingress_json();
    let text = value.to_string();
    let ingress = decode_presentation_ingress_v4_str(&text).expect("nulls decode");
    assert_eq!(ingress.foreground, "");
    assert_eq!(ingress.foreground_minimized, None);
    assert_eq!(ingress.foreground_frame, None);
    assert_eq!(ingress.witness, None);
    assert_eq!(ingress.foreground_out, None);
    let encoded = encode_presentation_ingress_v4(&ingress).expect("nulls encode");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).expect("fragment"),
        value
    );
    // Every nullable foreground fact accepts true/false/null.
    for fact in [
        "minimized",
        "maximized",
        "fullscreen",
        "hostHidden",
        "hiddenWorkspace",
    ] {
        for replacement in [
            serde_json::json!(true),
            serde_json::json!(false),
            serde_json::Value::Null,
        ] {
            let mut v = minimal_ingress_json();
            v[fact] = replacement;
            decode_presentation_ingress_v4_str(&v.to_string())
                .unwrap_or_else(|_| panic!("{fact} nullable"));
        }
        let mut wrong = minimal_ingress_json();
        wrong[fact] = serde_json::json!("false");
        assert_eq!(
            decode_presentation_ingress_v4_str(&wrong.to_string())
                .unwrap_err()
                .kind,
            "malformed",
            "{fact} wrong type refuses"
        );
    }
    // Empty witness ids refuse: both slots are always nonempty.
    for (key, bad) in [
        ("win", serde_json::json!("")),
        ("foreground", serde_json::json!("")),
    ] {
        let mut witness = serde_json::json!({
            "win": "win-a", "obs": "1",
            "frame": [0, 0, 8, 8], "foreground": "win-a",
        });
        witness[key] = bad;
        assert_eq!(
            decode_frame_witness_str(&witness.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "witness {key} empty refuses"
        );
    }
}

fn witness_json(win: &str, foreground: &str) -> String {
    serde_json::json!({
        "win": win, "obs": "1",
        "frame": [0, 0, 8, 8], "foreground": foreground,
    })
    .to_string()
}

#[test]
fn v4_id_length_and_alphabet_boundaries_hold_on_every_slot() {
    // Identity is per slot, not per shape: every carried id slot accepts a
    // 128-byte opaque token and refuses 129 bytes, empty, and bad-alphabet
    // spellings as `unknown-value`. The only empty-allowed slots are the
    // explicit-none ingress `foreground`/`moveWindow` tokens; every other
    // slot reuses the shared opaque gate with no empty sentinel. Shape
    // only: dangling ids stay accepted, no cross-slot join is claimed.
    let ok_128 = "w".repeat(128);
    let bad_129 = "w".repeat(129);
    let bad_alpha = ["not an id!", "o\u{00fc}t", "a/b"];
    fn renderer_input_json(kind: &str, out: &str, ws: &str, leaf: Option<(&str, &str)>) -> String {
        let mut target = serde_json::json!({"kind": kind, "out": out, "ws": ws});
        if let Some((key, value)) = leaf {
            target[key] = serde_json::Value::String(value.to_owned());
        }
        serde_json::json!({"rev": "1", "target": target}).to_string()
    }
    // Renderer arms x actual slots: workspace(out,ws), window(out,ws,win),
    // group(out,ws,group). Neighbour slots stay valid while the slot under
    // test varies.
    for (kind, leaf_key) in [
        ("workspace", None),
        ("window", Some("win")),
        ("group", Some("group")),
    ] {
        let leaf_ok = match leaf_key {
            Some("win") => "win-a",
            Some("group") => "group-a",
            _ => "",
        };
        let mut slots = vec!["out", "ws"];
        if let Some(extra) = leaf_key {
            slots.push(extra);
        }
        for slot in slots {
            let build = |candidate: &str| -> String {
                let (out, ws) = match slot {
                    "out" => (candidate.to_owned(), "ws-a".to_owned()),
                    "ws" => ("out-a".to_owned(), candidate.to_owned()),
                    _ => ("out-a".to_owned(), "ws-a".to_owned()),
                };
                let leaf = leaf_key.map(|key| {
                    if slot == key {
                        (key, candidate.to_owned())
                    } else {
                        (key, leaf_ok.to_owned())
                    }
                });
                match leaf {
                    Some((key, value)) => renderer_input_json(kind, &out, &ws, Some((key, &value))),
                    None => renderer_input_json(kind, &out, &ws, None),
                }
            };
            decode_renderer_input_str(&build(&ok_128))
                .unwrap_or_else(|_| panic!("renderer {kind}.{slot} 128 decodes"));
            let mut bads = vec![bad_129.clone(), String::new()];
            bads.extend(bad_alpha.iter().map(|s| s.to_string()));
            for bad in &bads {
                assert_eq!(
                    decode_renderer_input_str(&build(bad)).unwrap_err().kind,
                    "unknown-value",
                    "renderer {kind}.{slot} {bad:?} refuses"
                );
            }
        }
    }
    // Witness win and foreground slots.
    for slot in ["win", "foreground"] {
        let good = match slot {
            "win" => witness_json(&ok_128, "win-a"),
            _ => witness_json("win-a", &ok_128),
        };
        decode_frame_witness_str(&good).unwrap_or_else(|_| panic!("witness {slot} 128 decodes"));
        let mut bads = vec![bad_129.clone(), String::new()];
        bads.extend(bad_alpha.iter().map(|s| s.to_string()));
        for bad in &bads {
            let text = match slot {
                "win" => witness_json(bad, "win-a"),
                _ => witness_json("win-a", bad),
            };
            assert_eq!(
                decode_frame_witness_str(&text).unwrap_err().kind,
                "unknown-value",
                "witness {slot} {bad:?} refuses"
            );
        }
    }
    // Ingress foreground/moveWindow: 128 accepts; 129 and bad alphabets
    // refuse; empty stays allowed (explicit none).
    for key in ["foreground", "moveWindow"] {
        let mut v = minimal_ingress_json();
        v[key] = serde_json::Value::String(ok_128.clone());
        decode_presentation_ingress_v4_str(&v.to_string())
            .unwrap_or_else(|_| panic!("{key} 128 decodes"));
        let mut over = minimal_ingress_json();
        over[key] = serde_json::Value::String(bad_129.clone());
        assert_eq!(
            decode_presentation_ingress_v4_str(&over.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "{key} 129 refuses"
        );
        for bad in bad_alpha {
            let mut v = minimal_ingress_json();
            v[key] = serde_json::Value::String(bad.to_owned());
            assert_eq!(
                decode_presentation_ingress_v4_str(&v.to_string())
                    .unwrap_err()
                    .kind,
                "unknown-value",
                "{key} {bad:?} refuses"
            );
        }
    }
    // Empty foreground/move tokens stay allowed (explicit none).
    let empty = minimal_ingress_json();
    assert_eq!(
        decode_presentation_ingress_v4(&empty)
            .expect("empty decodes")
            .foreground,
        ""
    );
    // Ingress foregroundOut: null (minimal fixture row) or opaque only;
    // 128 accepts; empty, 129, and bad alphabets refuse.
    let mut v = minimal_ingress_json();
    v["foregroundOut"] = serde_json::Value::String(ok_128.clone());
    decode_presentation_ingress_v4_str(&v.to_string()).expect("foregroundOut 128 decodes");
    let mut bads = vec![bad_129.clone(), String::new()];
    bads.extend(bad_alpha.iter().map(|s| s.to_string()));
    for bad in &bads {
        let mut v = minimal_ingress_json();
        v["foregroundOut"] = serde_json::Value::String(bad.clone());
        assert_eq!(
            decode_presentation_ingress_v4_str(&v.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "foregroundOut {bad:?} refuses"
        );
    }
    // Ingress pagerOrder entries: 128 accepts; empty, 129, and bad
    // alphabets refuse (no empty convention outside foreground/moveWindow).
    let mut v = minimal_ingress_json();
    v["pagerOrder"] = serde_json::json!([ok_128]);
    decode_presentation_ingress_v4_str(&v.to_string()).expect("order 128 decodes");
    let mut bads = vec![bad_129.clone(), String::new()];
    bads.extend(bad_alpha.iter().map(|s| s.to_string()));
    for bad in &bads {
        let mut v = minimal_ingress_json();
        v["pagerOrder"] = serde_json::json!([bad]);
        assert_eq!(
            decode_presentation_ingress_v4_str(&v.to_string())
                .unwrap_err()
                .kind,
            "unknown-value",
            "pagerOrder {bad:?} refuses"
        );
    }
    // Pager row win slot: 128 accepts; 129, empty, and bad alphabets refuse.
    let good = serde_json::json!({"win": ok_128, "live": null}).to_string();
    decode_pager_frame_v4_str(&good).expect("pager win 128 decodes");
    let mut bads = vec![bad_129.clone(), String::new()];
    bads.extend(bad_alpha.iter().map(|s| s.to_string()));
    for bad in &bads {
        let text = serde_json::json!({"win": bad, "live": null}).to_string();
        assert_eq!(
            decode_pager_frame_v4_str(&text).unwrap_err().kind,
            "unknown-value",
            "pager win {bad:?} refuses"
        );
    }
    // Renderer arms: bad kind, extra arm keys, and unicode ids refuse.
    let bad_kind = serde_json::json!({
        "rev": "1",
        "target": {"kind": "border", "out": "out-a", "ws": "ws-a", "win": "win-a"},
    });
    assert_eq!(
        decode_renderer_input_str(&bad_kind.to_string())
            .unwrap_err()
            .kind,
        "unknown-value"
    );
    let extra_win = serde_json::json!({
        "rev": "1",
        "target": {"kind": "workspace", "out": "out-a", "ws": "ws-a", "win": "win-a"},
    });
    assert_eq!(
        decode_renderer_input_str(&extra_win.to_string())
            .unwrap_err()
            .kind,
        "unknown-field"
    );
    let unicode_out = serde_json::json!({
        "rev": "1",
        "target": {"kind": "workspace", "out": "o\u{00fc}t", "ws": "ws-a"},
    });
    assert_eq!(
        decode_renderer_input_str(&unicode_out.to_string())
            .unwrap_err()
            .kind,
        "unknown-value"
    );
}

#[test]
fn v4_must_keys_wrong_types_counters_rects_and_caps() {
    // Missing required keys refuse as malformed; unknown keys refuse as
    // unknown-field; `chordHeld` is never nullable.
    let mut missing = minimal_ingress_json();
    missing.as_object_mut().expect("object").remove("witness");
    assert_eq!(
        decode_presentation_ingress_v4_str(&missing.to_string())
            .unwrap_err()
            .kind,
        "malformed"
    );
    let mut extra = minimal_ingress_json();
    extra["paused"] = serde_json::Value::Bool(false);
    assert_eq!(
        decode_presentation_ingress_v4_str(&extra.to_string())
            .unwrap_err()
            .kind,
        "unknown-field"
    );
    let mut null_chord = minimal_ingress_json();
    null_chord["chordHeld"] = serde_json::Value::Null;
    assert_eq!(
        decode_presentation_ingress_v4_str(&null_chord.to_string())
            .unwrap_err()
            .kind,
        "malformed"
    );
    // Counters: 0/9/10/u64::MAX canonical; JSON numbers, leading zeros,
    // signs, empties, and overflow refuse. Geometry shares the rule.
    for good in ["0", "9", "10", "18446744073709551615"] {
        let witness = serde_json::json!({
            "win": "win-a", "obs": good,
            "frame": [0, 0, 8, 8], "foreground": "win-a",
        });
        assert_eq!(
            decode_frame_witness_str(&witness.to_string())
                .expect("counter decodes")
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
        assert!(
            decode_frame_witness_str(&bad.to_string()).is_err(),
            "obs {bad} refuses"
        );
    }
    // Rects: zero/negative extents and out-of-bound coords refuse as
    // unknown-value; wrong arity and non-integer types refuse as malformed.
    for bad_frame in [
        serde_json::json!([0, 0, 0, 8]),
        serde_json::json!([0, 0, -1, 8]),
        serde_json::json!([16385, 0, 8, 8]),
        serde_json::json!([0, 0, 16385, 8]),
    ] {
        let witness = serde_json::json!({
            "win": "win-a", "obs": "1",
            "frame": bad_frame, "foreground": "win-a",
        });
        assert_eq!(
            decode_frame_witness_str(&witness.to_string())
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
        let witness = serde_json::json!({
            "win": "win-a", "obs": "1",
            "frame": bad_frame, "foreground": "win-a",
        });
        assert_eq!(
            decode_frame_witness_str(&witness.to_string())
                .unwrap_err()
                .kind,
            "malformed",
            "frame {bad_frame} refuses"
        );
    }
    // Caps: 1024 order/frame rows decode; 1025 refuse with fixed tokens.
    for (array, token) in [
        ("pagerOrder", "presentation-order-over-cap"),
        ("pagerFrames", "presentation-frames-over-cap"),
    ] {
        let mut at_cap = minimal_ingress_json();
        at_cap[array] = if array == "pagerOrder" {
            serde_json::Value::Array(
                (0..1024)
                    .map(|i| serde_json::json!(format!("w-{i}")))
                    .collect(),
            )
        } else {
            serde_json::Value::Array(
                (0..1024)
                    .map(|_| serde_json::json!({"win": "w-1", "live": null}))
                    .collect(),
            )
        };
        decode_presentation_ingress_v4_str(&at_cap.to_string())
            .unwrap_or_else(|_| panic!("{array} 1024 decodes"));
        let mut over = minimal_ingress_json();
        over[array] = if array == "pagerOrder" {
            serde_json::Value::Array(
                (0..1025)
                    .map(|i| serde_json::json!(format!("w-{i}")))
                    .collect(),
            )
        } else {
            serde_json::Value::Array(
                (0..1025)
                    .map(|_| serde_json::json!({"win": "w-1", "live": null}))
                    .collect(),
            )
        };
        let error = decode_presentation_ingress_v4_str(&over.to_string()).unwrap_err();
        assert_eq!(error.kind, "snapshot-invalid", "{array} over cap");
        assert_eq!(error.message, token, "{array} fixed token");
    }
    // Duplicate keys refuse, including escaped-spelling duplicates.
    let dup = r#"{"win":"win-a","win":"win-b","obs":"1","frame":[0,0,8,8],"foreground":"win-a"}"#;
    assert_eq!(
        decode_frame_witness_str(dup).unwrap_err().kind,
        "malformed",
        "duplicate key refuses"
    );
    let escaped = "{\"win\":\"win-a\",\"\\u0077in\":\"win-b\",\"obs\":\"1\",\"frame\":[0,0,8,8],\"foreground\":\"win-a\"}";
    assert_eq!(
        decode_frame_witness_str(escaped).unwrap_err().kind,
        "malformed",
        "escaped duplicate refuses"
    );
    // Row input bound mirrors the current route: oversized refuses.
    let big = "x".repeat(WORLD_MAX_REQUEST_BYTES + 1);
    assert_eq!(
        decode_presentation_ingress_v4_str(&big).unwrap_err().kind,
        "oversized"
    );
    assert_eq!(
        decode_frame_witness_str(&big).unwrap_err().kind,
        "oversized"
    );
}

#[test]
fn v4_checked_encoders_reject_invalid_typed_rows() {
    // Encoders never emit unsafe JSON: invalid typed ids, nullability,
    // geometry, or over-cap rows return Err instead of encoding.
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
    assert!(encode_frame_witness(&witness("", "win-a")).is_err());
    assert!(encode_frame_witness(&witness("win-a", "")).is_err());
    assert!(encode_frame_witness(&witness("not an id!", "win-a")).is_err());
    assert!(
        encode_frame_witness(&FrameWitness {
            win: WindowId("win-a".to_owned()),
            obs: 1,
            frame: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 8
            },
            foreground: WindowId("win-a".to_owned()),
        })
        .is_err()
    );
    let pager = |win: &str| PresentationPagerFrameV4 {
        window: WindowId(win.to_owned()),
        live: None,
    };
    assert!(encode_pager_frame_v4(&pager("")).is_err());
    assert!(
        encode_render_target(&RenderTarget::Workspace {
            out: OutputId("o\u{00fc}t".to_owned()),
            ws: WorkspaceId("ws-a".to_owned()),
        })
        .is_err()
    );
    assert!(
        encode_renderer_input(&RendererInput {
            rev: 1,
            target: RenderTarget::Window {
                out: OutputId("out-a".to_owned()),
                ws: WorkspaceId("ws-a".to_owned()),
                win: WindowId(String::new()),
            },
        })
        .is_err()
    );
    // Over-cap typed rows never encode.
    let over = PresentationIngressV4 {
        foreground: String::new(),
        foreground_minimized: None,
        foreground_maximized: None,
        foreground_fullscreen: None,
        foreground_host_hidden: None,
        foreground_hidden_workspace: None,
        foreground_frame: None,
        chord_held: false,
        move_window: String::new(),
        pager_order: (0..1025).map(|i| WindowId(format!("w-{i}"))).collect(),
        pager_frames: Vec::new(),
        witness: None,
        foreground_out: None,
    };
    assert!(encode_presentation_ingress_v4(&over).is_err());
    // Well-formed but dangling/mismatched rows are shape-accepted and
    // round-trip: ids need not join the ingress or live reports here.
    let dangling = PresentationIngressV4 {
        foreground: "ghost-fg".to_owned(),
        foreground_minimized: Some(true),
        foreground_maximized: None,
        foreground_fullscreen: Some(false),
        foreground_host_hidden: None,
        foreground_hidden_workspace: Some(true),
        foreground_frame: Some(rect),
        chord_held: true,
        move_window: "ghost-move".to_owned(),
        pager_order: vec![WindowId("ghost-order".to_owned())],
        pager_frames: vec![PresentationPagerFrameV4 {
            window: WindowId("ghost-row".to_owned()),
            live: None,
        }],
        witness: Some(witness("ghost-win", "ghost-foreground")),
        foreground_out: Some(OutputId("ghost-out".to_owned())),
    };
    let encoded = encode_presentation_ingress_v4(&dangling).expect("dangling encodes");
    let reparsed = decode_presentation_ingress_v4_str(&encoded).expect("dangling decodes");
    assert_eq!(reparsed, dangling);
    // Unused-import guard: group arms keep the distinct NodeId type.
    let group = RenderTarget::Group {
        out: OutputId("out-a".to_owned()),
        ws: WorkspaceId("ws-a".to_owned()),
        group: NodeId("group-a".to_owned()),
    };
    let text = encode_render_target(&group).expect("group encodes");
    assert!(text.contains("\"group\":\"group-a\""));
}

#[test]
fn v4_existing_v3_gates_are_unchanged() {
    // The staged row codecs add no report fields and change no v3 behavior:
    // a minimal v3 observation-only envelope still decodes on both entry
    // points with contract version 3.
    let request = "{\"v\":3,\"adapter\":\"a\",\"gen\":\"gen-1\",\
         \"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\"cmdSeq\":null,\
         \"command\":null,\
         \"screens\":[{\"out\":\"out\",\"full\":[0,0,800,600],\"work\":[0,0,800,560],\
          \"complete\":true,\"scale\":1.0,\
          \"identity\":{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}],\
         \"windows\":[],\
         \"views\":[{\"out\":\"out\",\"view\":\"\"}],\"closed\":[],\"native\":[],\
         \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
         \"actionOutputs\":[\"out\"],\
         \"focused\":\"\",\"complete\":true,\"gesture\":false,\
         \"presentation\":{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
         \"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\"shell\":false,\
         \"frame\":null,\"borderEnabled\":false,\"underlayEnabled\":false,\
         \"borderGap\":0,\"borderWidth\":0,\"underlayExtension\":0,\
         \"chordHeld\":false,\"moveWindow\":\"\",\"paused\":false,\
         \"pagerOrder\":[],\"pagerFrames\":[]},\
         \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[]}"
        .to_owned();
    let report = decode_request_legacy(&request).expect("v3 decodes");
    assert_eq!(report.contract_version, 3);
    let wired = decode_request_with_wire_legacy(&request).expect("v3 wired decodes");
    assert_eq!(wired.report, report);
}
