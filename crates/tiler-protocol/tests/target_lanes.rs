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
    let mut decoded: DecodedReplyV4 =
        decode_reply_v4(&fixture_text("reply-observe")).expect("observe decodes");

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
        clear_operation: None,
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
    };
    let fullscreen_max = StateTarget {
        property: StateProperty::Fullscreen,
        value: false,
        authority: max_hold,
        command_seq: Some(u64::MAX),
    };
    let maximized_max = StateTarget {
        property: StateProperty::Maximized,
        value: false,
        authority: max_hold,
        command_seq: Some(u64::MAX),
    };
    let above_commandless_max = StateTarget {
        property: StateProperty::Above,
        value: false,
        authority: max_hold,
        command_seq: None,
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
                        },
                        StateTarget {
                            property: StateProperty::Above,
                            value: true,
                            authority,
                            command_seq: None,
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
fn v4_writer_spells_clear_op_and_untouched_target_lanes() {
    let mut decoded: DecodedReplyV4 =
        decode_reply_v4(&fixture_text("reply-observe")).expect("observe decodes");
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
        clear_operation: None,
        command_seq: None,
        visibility: None,
        visibility_authority: ScopeAuthority::Untouched,
        state: Vec::new(),
    };
    let cases = [
        (
            Some("op-13"),
            Some(13),
            "\"clear_op\":\"op-13\",\"cmdSeq\":\"13\",\"visibility\":null,\
             \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\"state\":[]",
        ),
        (
            None,
            Some(13),
            "\"clear_op\":\"\",\"cmdSeq\":\"13\",\"visibility\":null,\
             \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\"state\":[]",
        ),
        (
            Some("op-born-1"),
            None,
            "\"clear_op\":\"op-born-1\",\"cmdSeq\":null,\"visibility\":null,\
             \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\"state\":[]",
        ),
    ];
    for (clear_op, command_seq, expected_tail) in cases {
        let mut row = base();
        row.clear_operation = clear_op.map(str::to_owned);
        row.command_seq = command_seq;
        decoded.reply.windows = vec![row];
        let wire = encode_reply_v4(&decoded).expect("reply encodes");
        assert_eq!(
            wire.len(),
            count_reply_v4(&decoded.reply).expect("protocol count succeeds"),
        );
        assert_eq!(decode_reply_v4(&wire).expect("reply round-trips"), decoded,);
        assert!(
            wire.contains(expected_tail),
            "missing exact tail {expected_tail} in {wire}"
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
