//! Outbound target-lane bytes: the frozen per-window suffix after
//! `clear_op` (`cmdSeq`, `visibility`, `visibility_auth`,
//! `visibility_hold`, `state`).
//!
//! Two hooks: v4 writer evidence (the canonical v4 writer spells crafted
//! lanes with exact size accounting pinned by the independent core byte
//! count plus a decoder roundtrip) and independent fixture conformance
//! (actual code encoding of fixture target fields matches the raw
//! immutable suffix bytes).
//!
//! Offline only: no D-Bus, no KWin.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::world_report::{
    HoldReason, ScopeAuthority, StateProperty, StateTarget, VisibilityTarget, WindowDesire,
};
use tiler_core::world_sizing_v4::{reply_estimate_v4, reply_exact_bytes_v4};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, decode_reply_v4, encode_reply_v4,
};

fn fixture_text(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    std::fs::read_to_string(&path).expect("fixture bytes")
}

#[test]
fn v4_writer_spells_crafted_target_lanes_with_exact_bytes() {
    // Writer hook (only the shared lane helper is common to both sides,
    // never an independent oracle): a reply with non-default lanes
    // (visibility object, held authorities with the longest reason,
    // multi-row state with quoted correlations, and a boundary-length
    // lifetime proving the shared string accounting) spells through the
    // genuine v4 writer, with the independent core byte count exact, a
    // decoder roundtrip to the identical typed reply, and the structural
    // estimate covering it. The legacy escape-heavy lifetime is not
    // stageable here: the v4 opaque-id gate (`is_opaque_id`, `[A-Za-z0-9-_.]`
    // max 128, enforced by the v4 writer) refuses it as `unknown-value`.
    let mut decoded: DecodedReplyV4 =
        decode_reply_v4(&fixture_text("reply-observe")).expect("observe decodes");
    decoded.reply.windows = vec![
        WindowDesire {
            lifetime: WindowId("win-a".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Untouched,
            membership_authority: ScopeAuthority::Untouched,
            floating: false,
            float_origin: None,
            frame_operation: None,
            clear_operation: None,
            command_seq: None,
            visibility: None,
            visibility_authority: ScopeAuthority::Untouched,
            state: Vec::new(),
        },
        WindowDesire {
            lifetime: WindowId("a.9_".repeat(32)),
            output: OutputId("out".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Untouched,
            membership_authority: ScopeAuthority::Untouched,
            floating: false,
            float_origin: None,
            frame_operation: None,
            clear_operation: None,
            command_seq: Some(u64::MAX),
            visibility: Some(VisibilityTarget {
                shown: false,
                revision: u64::MAX,
                fallback: true,
            }),
            visibility_authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            state: vec![
                StateTarget {
                    property: StateProperty::Above,
                    value: true,
                    authority: ScopeAuthority::Controlled,
                    command_seq: None,
                },
                StateTarget {
                    property: StateProperty::Fullscreen,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::RefusedTarget),
                    command_seq: Some(1),
                },
                StateTarget {
                    property: StateProperty::Maximized,
                    value: true,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
            ],
        },
    ];
    decoded.wire = ReplyWireOptionals {
        suspended_output_present: decoded.reply.presentation.suspended_output.is_some(),
    };
    let wire = encode_reply_v4(&decoded).expect("reply encodes");
    assert_eq!(
        wire.len(),
        reply_exact_bytes_v4(&decoded.reply),
        "core byte count is exact over crafted lanes"
    );
    assert_eq!(
        decode_reply_v4(&wire).expect("reply round-trips"),
        decoded,
        "wire round-trips to the identical typed reply and metadata"
    );
    assert!(
        reply_estimate_v4(&decoded.reply) >= wire.len(),
        "structural estimate covers crafted lanes"
    );
    let mut lanes = String::new();
    tiler_core::world_sizing::encode_window_target_lanes(&mut lanes, &decoded.reply.windows[1]);
    assert_eq!(
        tiler_core::world_sizing::window_target_lanes_estimate(&decoded.reply.windows[1]),
        lanes.len(),
        "lane estimate stays exact on crafted lanes"
    );
    assert!(lanes.contains("\"cmdSeq\":\"18446744073709551615\""));
    assert!(lanes.contains(
        "\"visibility\":{\"shown\":false,\"rev\":\"18446744073709551615\",\"fallback\":true}"
    ));
    assert!(lanes.contains("\"visibility_auth\":\"hold\""));
    assert!(lanes.contains("\"visibility_hold\":\"incomplete-inventory\""));
    assert!(lanes.contains(
        "{\"property\":\"maximized\",\"value\":true,\"auth\":\"hold\",\
         \"reason\":\"incomplete-inventory\",\"cmdSeq\":\"18446744073709551615\"}"
    ));
    assert!(
        wire.contains(&format!("\"win\":\"{}\"", "a.9_".repeat(32))),
        "boundary-length lifetime rides the wire byte-exact"
    );
}

#[test]
fn immutable_fixtures_decode_to_code_spelled_target_suffix() {
    // Independent conformance: raw immutable bytes on one side, actual
    // code encoding on the other. For each fixture's first window row,
    // map ONLY the target fields into a WindowDesire test default
    // (test-only token mappings, never a production decoder), encode the
    // suffix with the real writer, and compare against the raw suffix
    // bytes sliced from the fixture. No serde serialization on the
    // expected side: key order would be lost.
    for name in [
        "reply-observe",
        "reply-intent-save-clear",
        "reply-above-true",
        "reply-above-false",
        "reply-visibility-hide-staged",
        "reply-visibility-hide-fallback",
        "reply-visibility-independent",
        "reply-state-authority",
    ] {
        let wire = fixture_text(name);
        let value: serde_json::Value = serde_json::from_str(&wire).expect("fixture parses");
        let row = value
            .get("windows")
            .and_then(|windows| windows.as_array())
            .and_then(|windows| windows.first())
            .unwrap_or_else(|| panic!("{name} carries a first window row"));
        let desire = WindowDesire {
            lifetime: WindowId("test-only".to_owned()),
            output: OutputId("test-only".to_owned()),
            workspace: String::new(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Untouched,
            membership_authority: ScopeAuthority::Untouched,
            floating: false,
            float_origin: None,
            frame_operation: None,
            clear_operation: None,
            command_seq: test_opt_cmdseq(row.get("cmdSeq"), name),
            visibility: match row.get("visibility") {
                None | Some(serde_json::Value::Null) => None,
                Some(visibility) => Some(VisibilityTarget {
                    shown: visibility
                        .get("shown")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} visibility shown")),
                    revision: visibility
                        .get("rev")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_else(|| panic!("{name} visibility rev"))
                        .parse::<u64>()
                        .unwrap_or_else(|_| panic!("{name} visibility rev parses")),
                    fallback: visibility
                        .get("fallback")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} visibility fallback")),
                }),
            },
            visibility_authority: test_authority(
                row.get("visibility_auth")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("{name} visibility_auth")),
                row.get("visibility_hold")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("{name} visibility_hold")),
                name,
            ),
            state: row
                .get("state")
                .and_then(serde_json::Value::as_array)
                .unwrap_or_else(|| panic!("{name} state array"))
                .iter()
                .map(|target| StateTarget {
                    property: match target.get("property").and_then(serde_json::Value::as_str) {
                        Some("fullscreen") => StateProperty::Fullscreen,
                        Some("maximized") => StateProperty::Maximized,
                        Some("sticky") => StateProperty::Sticky,
                        Some("above") => StateProperty::Above,
                        other => panic!("{name} test-only state property: {other:?}"),
                    },
                    value: target
                        .get("value")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} state value")),
                    authority: test_authority(
                        target
                            .get("auth")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_else(|| panic!("{name} state auth")),
                        target
                            .get("reason")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_else(|| panic!("{name} state reason")),
                        name,
                    ),
                    command_seq: test_opt_cmdseq(target.get("cmdSeq"), name),
                })
                .collect(),
        };
        // Encode with the real writer: lane fields only.
        let mut actual = String::new();
        tiler_core::world_sizing::encode_window_target_lanes(&mut actual, &desire);
        // Raw suffix slice: the reply's first `,"cmdSeq":` opens the first
        // window's suffix (no row boundary between the windows array open
        // and it), and the first `]}` after it closes the state array
        // plus the window (no earlier array inside the suffix). Take
        // through `]`, excluding the window brace.
        let windows_open = wire
            .find("\"windows\":[")
            .unwrap_or_else(|| panic!("{name} windows array"));
        let start = wire
            .find(",\"cmdSeq\":")
            .unwrap_or_else(|| panic!("{name} cmdSeq lane"));
        assert!(
            start > windows_open,
            "{name} suffix slice starts inside the windows array"
        );
        assert!(
            !wire[windows_open..start].contains("},{"),
            "{name} suffix slice is the first window row"
        );
        let after = &wire[start..];
        let end = after
            .find("]}")
            .unwrap_or_else(|| panic!("{name} state close plus window close"));
        assert_eq!(
            after[..end].matches('[').count(),
            1,
            "{name} no earlier array inside the suffix"
        );
        let raw = &after[..end + 1];
        assert_eq!(
            actual, raw,
            "{name} code spelling matches the raw immutable suffix"
        );
    }
}

/// Test-only `cmdSeq` lane mapping: JSON null or a bounded decimal
/// string. Never a production decoder.
fn test_opt_cmdseq(value: Option<&serde_json::Value>, name: &str) -> Option<u64> {
    match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => Some(
            text.parse::<u64>()
                .unwrap_or_else(|_| panic!("{name} cmdSeq parses: {text:?}")),
        ),
        Some(other) => panic!("{name} test-only cmdSeq shape: {other:?}"),
    }
}

/// Test-only authority mapping: the token plus its hold-reason lane.
/// Never a production decoder.
fn test_authority(auth: &str, reason: &str, name: &str) -> ScopeAuthority {
    match auth {
        "controlled" => ScopeAuthority::Controlled,
        "untouched" => ScopeAuthority::Untouched,
        "hold" => ScopeAuthority::Hold(match reason {
            "unreadable" => HoldReason::Unreadable,
            "incomplete-inventory" => HoldReason::IncompleteInventory,
            "gesture-barrier" => HoldReason::GestureBarrier,
            "refused-target" => HoldReason::RefusedTarget,
            "ambiguous-creation" => HoldReason::AmbiguousCreation,
            "occupied-removal" => HoldReason::OccupiedRemoval,
            "constraint-clamp" => HoldReason::ConstraintClamp,
            "minimized" => HoldReason::Minimized,
            other => panic!("{name} test-only hold reason: {other:?}"),
        }),
        other => panic!("{name} test-only authority: {other:?}"),
    }
}
