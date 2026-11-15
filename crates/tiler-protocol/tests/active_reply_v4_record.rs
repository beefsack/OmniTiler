//! Typed reply-schema recorder for `test-fixtures/world-wire/v4-active/messages`.
//!
//! Valid and handshake-contextual witnesses start as explicit Rust values in
//! `valid_cases`, never decoded old fixture replies. Production `encode_reply_v4`
//! supplies their bytes. Context-free negatives change one declared pointer in
//! a freshly encoded typed witness; exact refusals and causal repairs are checked.
//! These are schema witnesses, not claims of engine-produced behavior.
//!
//! Normal runs compare committed bytes with the same producers. Re-record only
//! with `UPDATE_ACTIVE_REPLY_V4_FIXTURES=1 cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`.
//! Only local WorldReply message paths in the active manifest can be written.

#[path = "active_reply_v4_record/invalid_cases.rs"]
mod invalid_cases;
#[path = "active_reply_v4_record/valid_cases.rs"]
mod valid_cases;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tiler_protocol::world_protocol::{decode_reply_v4, encode_reply_v4};

#[derive(serde::Deserialize)]
struct Manifest {
    fixtures: Vec<Fixture>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    id: String,
    file: String,
    family: String,
    expectation: String,
    context: Option<serde_json::Value>,
    invalid: Option<Invalid>,
}

#[derive(serde::Deserialize)]
struct Invalid {
    source: Option<String>,
    pointer: Option<String>,
}

#[test]
fn active_reply_witnesses_match_typed_recorder() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4-active");
    let manifest: Manifest =
        serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
    let fixtures: Vec<_> = manifest
        .fixtures
        .into_iter()
        .filter(|fixture| fixture.family == "WorldReply")
        .collect();
    assert_eq!(fixtures.len(), 94);

    let valid = valid_cases::valid_reply_cases();
    let contextual = valid_cases::contextual_reject_cases();
    assert_eq!(valid.len(), 73);
    assert_eq!(contextual.len(), 3);
    let mut produced = BTreeMap::new();
    for (cases, expectation, require_context) in [
        (valid, "valid-wire", false),
        (contextual, "reject-wire", true),
    ] {
        for (id, typed) in cases {
            let fixture = fixtures.iter().find(|f| f.id == id).expect(id);
            assert_eq!(fixture.expectation, expectation, "{id}");
            if require_context {
                assert!(fixture.context.is_some(), "{id}");
            }
            let wire =
                encode_reply_v4(&typed).unwrap_or_else(|error| panic!("{id} encode: {error:?}"));
            assert_eq!(
                decode_reply_v4(&wire).unwrap_or_else(|error| panic!("{id} decode: {error:?}")),
                typed,
                "{id} typed round-trip"
            );
            assert!(produced.insert(id.to_owned(), wire).is_none(), "{id}");
        }
    }
    let negatives: Vec<_> = fixtures
        .iter()
        .filter(|f| f.expectation == "reject-wire" && f.context.is_none())
        .collect();
    assert_eq!(negatives.len(), 18);
    for fixture in negatives {
        let invalid = fixture.invalid.as_ref().expect("negative metadata");
        let source_id = invalid.source.as_deref().expect("negative source");
        let pointer = invalid.pointer.as_deref().expect("negative pointer");
        let source = produced.get(source_id).expect("fresh typed source");
        let wire = invalid_cases::produce(&fixture.id, source, pointer);
        assert!(produced.insert(fixture.id.clone(), wire).is_none());
    }
    assert_eq!(produced.len(), fixtures.len());

    // Validate the complete write set before any file is changed.
    for fixture in &fixtures {
        assert!(produced.contains_key(&fixture.id), "{}", fixture.id);
        let path = Path::new(&fixture.file);
        assert_eq!(path.parent(), Some(Path::new("messages")));
        assert_eq!(path.extension().and_then(|s| s.to_str()), Some("json"));
        assert!(path.components().all(|c| matches!(c, Component::Normal(_))));
        assert_eq!(fixture.file, format!("messages/{}.json", fixture.id));
    }
    let update = std::env::var("UPDATE_ACTIVE_REPLY_V4_FIXTURES").as_deref() == Ok("1");
    for fixture in fixtures {
        let wire = &produced[&fixture.id];
        let path = root.join(&fixture.file);
        if update {
            fs::write(&path, wire).unwrap();
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            *wire,
            "{} bytes must match typed recorder",
            fixture.id
        );
    }
}
