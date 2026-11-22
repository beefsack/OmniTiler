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
use tiler_protocol::world_admission_sizing::MAX_TARGET_LANES_PER_WINDOW;
use tiler_protocol::world_protocol::{DecodedReplyV4, decode_reply_v4, encode_reply_v4};
use tiler_protocol::world_reply_write::count_reply_v4;

fn fixture_text(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4-active/messages")
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
    let mut observe_json: serde_json::Value =
        serde_json::from_str(&fixture_text("reply-observe")).expect("observe json");
    if let Some(windows) = observe_json
        .get_mut("windows")
        .and_then(serde_json::Value::as_array_mut)
    {
        for row in windows {
            if let Some(obj) = row.as_object_mut() {
                obj.remove("clear_op");
            }
        }
    }
    let mut decoded: DecodedReplyV4 =
        decode_reply_v4(&observe_json.to_string()).expect("observe decodes");
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
                    automatic_clear: false,
                },
                StateTarget {
                    property: StateProperty::Fullscreen,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::RefusedTarget),
                    command_seq: Some(1),
                    automatic_clear: false,
                },
                StateTarget {
                    property: StateProperty::Maximized,
                    value: true,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                    automatic_clear: false,
                },
            ],
        },
    ];
    let wire = encode_reply_v4(&decoded).expect("reply encodes");
    assert_eq!(
        wire.len(),
        count_reply_v4(&decoded.reply).expect("protocol count succeeds"),
        "protocol byte count is exact over crafted lanes"
    );
    assert_eq!(
        decode_reply_v4(&wire).expect("reply round-trips"),
        decoded,
        "wire round-trips to the identical typed reply and metadata"
    );
    let win1 = wire
        .find(&format!("\"win\":\"{}\"", "a.9_".repeat(32)))
        .expect("win1 in wire");
    let start = win1 + wire[win1..].find(",\"cmdSeq\":").expect("win1 cmdSeq");
    let end = start + wire[start..].find("]}").expect("win1 state close");
    let lanes = &wire[start..end + 1];
    assert!(
        lanes.len() <= MAX_TARGET_LANES_PER_WINDOW,
        "crafted lanes {} bounded by per-window reserve {}",
        lanes.len(),
        MAX_TARGET_LANES_PER_WINDOW
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
fn max_legal_target_lane_width_combinations_match_exact_bytes() {
    let mut observe_json: serde_json::Value =
        serde_json::from_str(&fixture_text("reply-observe")).expect("observe json");
    if let Some(windows) = observe_json
        .get_mut("windows")
        .and_then(serde_json::Value::as_array_mut)
    {
        for row in windows {
            if let Some(obj) = row.as_object_mut() {
                obj.remove("clear_op");
            }
        }
    }
    let mut decoded: DecodedReplyV4 =
        decode_reply_v4(&observe_json.to_string()).expect("observe decodes");

    let lane_window = |command_seq, visibility, visibility_authority, state| WindowDesire {
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
        command_seq,
        visibility,
        visibility_authority,
        state,
    };

    let measure_lanes = |decoded: &mut DecodedReplyV4, window: WindowDesire| -> usize {
        decoded.reply.windows = vec![window];
        let wire = encode_reply_v4(decoded).expect("reply encodes");
        assert_eq!(
            wire.len(),
            count_reply_v4(&decoded.reply).expect("protocol count succeeds"),
            "protocol byte count exact"
        );
        assert_eq!(
            decode_reply_v4(&wire).expect("reply round-trips"),
            *decoded,
            "wire round-trips to identical reply"
        );
        let start = wire.find(",\"cmdSeq\":").expect("cmdSeq lane");
        let end = start + wire[start..].find("]}").expect("state close");
        wire[start..end + 1].len()
    };

    // Engine defaults: null correlation (94) and u64::MAX correlation (112).
    assert_eq!(
        measure_lanes(
            &mut decoded,
            lane_window(None, None, ScopeAuthority::Untouched, Vec::new()),
        ),
        94
    );
    assert_eq!(
        measure_lanes(
            &mut decoded,
            lane_window(Some(u64::MAX), None, ScopeAuthority::Untouched, Vec::new()),
        ),
        112
    );

    // Common controlled show with empty state: 32 + 74 + 31 + 21 + 11 = 169.
    let max_show = Some(VisibilityTarget {
        shown: true,
        revision: u64::MAX,
        fallback: false,
    });
    assert_eq!(
        measure_lanes(
            &mut decoded,
            lane_window(
                Some(u64::MAX),
                max_show,
                ScopeAuthority::Controlled,
                Vec::new()
            ),
        ),
        169
    );

    let max_hold = ScopeAuthority::Hold(HoldReason::IncompleteInventory);
    let sticky_max = StateTarget {
        property: StateProperty::Sticky,
        value: false,
        authority: max_hold,
        command_seq: Some(u64::MAX),
        automatic_clear: false,
    };
    let fullscreen_max = StateTarget {
        property: StateProperty::Fullscreen,
        value: false,
        authority: max_hold,
        command_seq: Some(u64::MAX),
        automatic_clear: false,
    };
    let maximized_max = StateTarget {
        property: StateProperty::Maximized,
        value: false,
        authority: max_hold,
        command_seq: Some(u64::MAX),
        automatic_clear: false,
    };
    let above_commandless_max = StateTarget {
        property: StateProperty::Above,
        value: false,
        authority: max_hold,
        command_seq: None,
        automatic_clear: false,
    };

    // 1-row sticky-only max: 296.
    assert_eq!(
        measure_lanes(
            &mut decoded,
            lane_window(Some(u64::MAX), max_show, max_hold, vec![sticky_max],),
        ),
        296
    );
    // 2-row sticky + fullscreen max: 414.
    assert_eq!(
        measure_lanes(
            &mut decoded,
            lane_window(
                Some(u64::MAX),
                max_show,
                max_hold,
                vec![sticky_max, fullscreen_max],
            ),
        ),
        414
    );
    // 3-row sticky + fullscreen + maximized max: 531.
    assert_eq!(
        measure_lanes(
            &mut decoded,
            lane_window(
                Some(u64::MAX),
                max_show,
                max_hold,
                vec![sticky_max, fullscreen_max, maximized_max,],
            ),
        ),
        531
    );
    // 4-row max legal combination (commandless above + three state rows): 626 == MAX_TARGET_LANES_PER_WINDOW.
    let four_row_len = measure_lanes(
        &mut decoded,
        lane_window(
            Some(u64::MAX),
            max_show,
            max_hold,
            vec![
                sticky_max,
                fullscreen_max,
                maximized_max,
                above_commandless_max,
            ],
        ),
    );
    assert_eq!(four_row_len, 626);
    assert_eq!(MAX_TARGET_LANES_PER_WINDOW, 626);

    // Authority and property matrix: every authority/hold reason, property, and seq boundary round-trips and counts exact.
    let authorities = [
        ScopeAuthority::Controlled,
        ScopeAuthority::Untouched,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        ScopeAuthority::Hold(HoldReason::IncompleteInventory),
        ScopeAuthority::Hold(HoldReason::GestureBarrier),
        ScopeAuthority::Hold(HoldReason::RefusedTarget),
        ScopeAuthority::Hold(HoldReason::AmbiguousCreation),
        ScopeAuthority::Hold(HoldReason::OccupiedRemoval),
        ScopeAuthority::Hold(HoldReason::ConstraintClamp),
        ScopeAuthority::Hold(HoldReason::Minimized),
    ];
    for authority in authorities {
        for seq in [None, Some(0), Some(u64::MAX)] {
            let len = measure_lanes(
                &mut decoded,
                lane_window(
                    seq,
                    Some(VisibilityTarget {
                        shown: false,
                        revision: u64::MAX,
                        fallback: true,
                    }),
                    authority,
                    vec![
                        StateTarget {
                            property: StateProperty::Fullscreen,
                            value: false,
                            authority,
                            command_seq: seq,
                            automatic_clear: false,
                        },
                        StateTarget {
                            property: StateProperty::Above,
                            value: true,
                            authority,
                            command_seq: None,
                            automatic_clear: false,
                        },
                    ],
                ),
            );
            assert!(
                len <= MAX_TARGET_LANES_PER_WINDOW,
                "lane width {len} exceeds reserve {MAX_TARGET_LANES_PER_WINDOW}"
            );
        }
    }
}

#[test]
fn v4_writer_spells_automatic_clear_state_target_and_rejects_invalid_combinations() {
    let mut observe_json: serde_json::Value =
        serde_json::from_str(&fixture_text("reply-observe")).expect("observe json");
    if let Some(windows) = observe_json
        .get_mut("windows")
        .and_then(serde_json::Value::as_array_mut)
    {
        for row in windows {
            if let Some(obj) = row.as_object_mut() {
                obj.remove("clear_op");
            }
        }
    }
    let mut decoded: DecodedReplyV4 =
        decode_reply_v4(&observe_json.to_string()).expect("observe decodes");
    let base = || WindowDesire {
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
        command_seq: None,
        visibility: None,
        visibility_authority: ScopeAuthority::Untouched,
        state: Vec::new(),
    };
    // Valid automaticClear: true on maximized=false (directional with cmdSeq and born without).
    for (command_seq, expected_state) in [
        (
            Some(13),
            "\"state\":[{\"property\":\"maximized\",\"value\":false,\
             \"auth\":\"controlled\",\"reason\":\"\",\"cmdSeq\":\"13\",\"automaticClear\":true}]",
        ),
        (
            None,
            "\"state\":[{\"property\":\"maximized\",\"value\":false,\
             \"auth\":\"controlled\",\"reason\":\"\",\"cmdSeq\":null,\"automaticClear\":true}]",
        ),
    ] {
        let mut row = base();
        row.command_seq = command_seq;
        row.state.push(StateTarget {
            property: StateProperty::Maximized,
            value: false,
            authority: ScopeAuthority::Controlled,
            command_seq,
            automatic_clear: true,
        });
        decoded.reply.windows = vec![row];
        let wire = encode_reply_v4(&decoded).expect("reply encodes");
        assert_eq!(
            wire.len(),
            count_reply_v4(&decoded.reply).expect("protocol count succeeds"),
        );
        assert_eq!(decode_reply_v4(&wire).expect("reply round-trips"), decoded);
        assert!(
            wire.contains(expected_state),
            "missing exact state {expected_state} in {wire}"
        );
        assert!(
            !wire.contains("\"clear_op\""),
            "clear_op must be absent: {wire}"
        );
    }
    // Ordinary maximized=false omits automaticClear.
    let mut ordinary = base();
    ordinary.state.push(StateTarget {
        property: StateProperty::Maximized,
        value: false,
        authority: ScopeAuthority::Controlled,
        command_seq: Some(13),
        automatic_clear: false,
    });
    decoded.reply.windows = vec![ordinary];
    let ordinary_wire = encode_reply_v4(&decoded).expect("ordinary encodes");
    assert!(
        !ordinary_wire.contains("automaticClear"),
        "omitted for ordinary: {ordinary_wire}"
    );

    // Writer rejects automatic_clear=true for maximized=true and non-maximized properties.
    for (property, value) in [
        (StateProperty::Maximized, true),
        (StateProperty::Fullscreen, false),
        (StateProperty::Sticky, false),
        (StateProperty::Above, false),
    ] {
        let mut bad = base();
        bad.state.push(StateTarget {
            property,
            value,
            authority: ScopeAuthority::Controlled,
            command_seq: None,
            automatic_clear: true,
        });
        decoded.reply.windows = vec![bad];
        assert!(
            encode_reply_v4(&decoded).is_err(),
            "writer must reject automatic_clear on {property:?}={value}"
        );
    }

    // Strict reader rejects automaticClear: false, null, non-bool, maximized=true, and non-maximized.
    let mut valid_row = base();
    valid_row.state.push(StateTarget {
        property: StateProperty::Maximized,
        value: false,
        authority: ScopeAuthority::Controlled,
        command_seq: None,
        automatic_clear: true,
    });
    decoded.reply.windows = vec![valid_row];
    let valid_wire = encode_reply_v4(&decoded).expect("valid encodes");
    for mut_fn in [
        Box::new(|s: &mut serde_json::Value| {
            s["automaticClear"] = serde_json::json!(false);
        }) as Box<dyn Fn(&mut serde_json::Value)>,
        Box::new(|s: &mut serde_json::Value| {
            s["automaticClear"] = serde_json::Value::Null;
        }),
        Box::new(|s: &mut serde_json::Value| {
            s["automaticClear"] = serde_json::json!("true");
        }),
        Box::new(|s: &mut serde_json::Value| {
            s["value"] = serde_json::json!(true);
        }),
        Box::new(|s: &mut serde_json::Value| {
            s["property"] = serde_json::json!("fullscreen");
        }),
    ] {
        let mut doc: serde_json::Value = serde_json::from_str(&valid_wire).expect("json");
        mut_fn(&mut doc["windows"][0]["state"][0]);
        assert!(
            decode_reply_v4(&doc.to_string()).is_err(),
            "reader must reject invalid automaticClear: {}",
            doc["windows"][0]["state"][0]
        );
    }
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
                    automatic_clear: target
                        .get("automaticClear")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                })
                .collect(),
        };
        // Encode with the genuine v4 reply writer and slice the actual
        // target-lane subsection from the first window row.
        let mut decoded = decode_reply_v4(&wire).expect("fixture decodes");
        let mut desire = desire;
        desire.lifetime = decoded.reply.windows[0].lifetime.clone();
        desire.output = decoded.reply.windows[0].output.clone();
        desire.workspace = decoded.reply.windows[0].workspace.clone();
        decoded.reply.windows = vec![desire];
        let encoded = encode_reply_v4(&decoded).expect("reply encodes");
        let enc_windows_open = encoded
            .find("\"windows\":[")
            .unwrap_or_else(|| panic!("{name} encoded windows array"));
        let enc_start = encoded
            .find(",\"cmdSeq\":")
            .unwrap_or_else(|| panic!("{name} encoded cmdSeq lane"));
        assert!(enc_start > enc_windows_open);
        let enc_after = &encoded[enc_start..];
        let enc_end = enc_after
            .find("]}")
            .unwrap_or_else(|| panic!("{name} encoded state close"));
        let actual = &enc_after[..enc_end + 1];
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
