//! Native send trace-line tests: the metadata-only leg/event/readback
//! lines carry exact IDs, geometry, hex chords, and elapsed timing — never
//! titles, content, or paths. Every line joins the fixed product route plus
//! the operation identity and both clocks (leg-relative `t_ms` plus the
//! process-common `now_ms`), so admission, proof, drag, release, settle,
//! topology, reread, and reconcile phases correlate across threads.
//! No live touch: every line is pure over explicit inputs.

use crate::space_drag_transfer::DragStep;
use crate::workspace_send_native::{
    SEND_ROUTE_BRIDGED, SEND_ROUTE_DRAG, format_native_key_line, format_native_leg_begin_line,
    format_native_mouse_line, format_native_phase_line, format_native_readback_line,
    native_step_name, send_now_ms,
};

fn frame() -> crate::ffi::AxFrame {
    crate::ffi::AxFrame {
        x: 100.0,
        y: 200.0,
        w: 800.0,
        h: 600.0,
    }
}

#[test]
fn step_names_match_the_diagnostic_kind_vocabulary() {
    assert_eq!(native_step_name(DragStep::MouseMove), "mouse-move");
    assert_eq!(native_step_name(DragStep::MouseDown), "mouse-down");
    assert_eq!(native_step_name(DragStep::MouseDragged), "mouse-dragged");
    assert_eq!(native_step_name(DragStep::MouseUp), "mouse-up");
    // Chord steps never travel the mouse vocabulary here.
    assert_eq!(native_step_name(DragStep::ChordDown), "mouse-unknown");
    assert_eq!(native_step_name(DragStep::ChordUp), "mouse-unknown");
}

#[test]
fn product_routes_are_fixed_drag_and_bridged() {
    // Drag is the legacy route; bridged is the fast route (exactly one
    // direct native move plus the Dock-swipe exact-SID selection, no
    // mouse/key posts). Both are fixed literals so log joins never guess.
    assert_eq!(SEND_ROUTE_DRAG, "drag");
    assert_eq!(SEND_ROUTE_BRIDGED, "bridged");
}

#[test]
fn bridged_phase_lines_join_route_op_revision_and_both_clocks() {
    assert_eq!(
        format_native_phase_line("bridged", "dispatch", "op-3", Some(77), 12, 9112),
        "tiler-macos send route=bridged phase=dispatch op=op-3 revision=Some(77) t_ms=12 now_ms=9112",
    );
    assert_eq!(
        format_native_phase_line("bridged", "select", "op-3", Some(77), 200, 9300),
        "tiler-macos send route=bridged phase=select op=op-3 revision=Some(77) t_ms=200 now_ms=9300",
    );
    assert_eq!(
        format_native_phase_line("bridged", "arrival", "op-3", Some(77), 900, 9900),
        "tiler-macos send route=bridged phase=arrival op=op-3 revision=Some(77) t_ms=900 now_ms=9900",
    );
}

#[test]
fn phase_line_joins_route_phase_op_revision_and_both_clocks() {
    assert_eq!(
        format_native_phase_line("drag", "proof", "op-3", Some(77), 0, 9100),
        "tiler-macos send route=drag phase=proof op=op-3 revision=Some(77) t_ms=0 now_ms=9100",
    );
    assert_eq!(
        format_native_phase_line("drag", "reread", "op-3", Some(77), 34, 9134),
        "tiler-macos send route=drag phase=reread op=op-3 revision=Some(77) t_ms=34 now_ms=9134",
    );
    // Pre-selection topology lines carry no op, no leg clock, and no route
    // claim (the owner chooses bridged-first with drag fallback later).
    assert_eq!(
        format_native_phase_line("unselected", "topology", "none", Some(77), 0, 9000),
        "tiler-macos send route=unselected phase=topology op=none revision=Some(77) t_ms=0 now_ms=9000",
    );
    // Unknown revision renders explicit (never guessed).
    assert_eq!(
        format_native_phase_line("drag", "reconcile", "op-3", None, 1200, 9200),
        "tiler-macos send route=drag phase=reconcile op=op-3 revision=None t_ms=1200 now_ms=9200",
    );
}

#[test]
fn leg_begin_pins_goal_grab_frame_and_relative_offset() {
    assert_eq!(
        format_native_leg_begin_line(277, (500.0, 400.0), frame(), "op-3", Some(77), 0, 9100),
        "tiler-macos send native route=drag phase=drag-start op=op-3 revision=Some(77) goal=277 grab=(500,400) frame=(100,200,800,600) rel=(400,200) t_ms=0 now_ms=9100",
    );
}

#[test]
fn mouse_line_carries_elapsed_point_and_frame_relative_offset() {
    assert_eq!(
        format_native_mouse_line(
            "mouse-down",
            12,
            (500.0, 400.0),
            Some(frame()),
            "op-3",
            9112
        ),
        "tiler-macos send native route=drag phase=drag ev=mouse-down op=op-3 t_ms=12 now_ms=9112 pt=(500,400) rel=(400,200)",
    );
    // Frameless posts still trace the point (never a guessed relative).
    assert_eq!(
        format_native_mouse_line("mouse-move", 3, (500.0, 400.0), None, "op-3", 9103),
        "tiler-macos send native route=drag phase=drag ev=mouse-move op=op-3 t_ms=3 now_ms=9103 pt=(500,400)",
    );
}

#[test]
fn key_line_carries_elapsed_hex_chord() {
    assert_eq!(
        format_native_key_line("chord-down", 63, 0x13, 0x0004_0000, "op-3", 9163),
        "tiler-macos send native route=drag phase=chord ev=chord-down op=op-3 t_ms=63 now_ms=9163 key=0x13 flags=0x40000",
    );
    assert_eq!(
        format_native_key_line("key-up", 470, 0x13, 0x0004_0000, "op-3", 9570),
        "tiler-macos send native route=drag phase=chord ev=key-up op=op-3 t_ms=470 now_ms=9570 key=0x13 flags=0x40000",
    );
}

#[test]
fn readback_line_distinguishes_baseline_from_change() {
    assert_eq!(
        format_native_readback_line(
            "drag",
            900,
            &Some(vec![1, 277]),
            &Some(1),
            false,
            "op-3",
            Some(77),
            9900
        ),
        "tiler-macos send native route=drag phase=settle op=op-3 revision=Some(77) t_ms=900 now_ms=9900 membership=Some([1, 277]) current=Some(1) changed=false",
    );
    assert_eq!(
        format_native_readback_line(
            "drag",
            1100,
            &Some(vec![277]),
            &Some(277),
            true,
            "op-3",
            Some(77),
            10100
        ),
        "tiler-macos send native route=drag phase=settle op=op-3 revision=Some(77) t_ms=1100 now_ms=10100 membership=Some([277]) current=Some(277) changed=true",
    );
    // Unreadable legs render as None (unknown, never guessed).
    assert_eq!(
        format_native_readback_line("drag", 3000, &None, &None, true, "op-3", None, 12000),
        "tiler-macos send native route=drag phase=settle op=op-3 revision=None t_ms=3000 now_ms=12000 membership=None current=None changed=true",
    );
    // The fast route joins on bridged, never a hardcoded drag.
    assert_eq!(
        format_native_readback_line(
            "bridged",
            900,
            &Some(vec![277]),
            &Some(277),
            false,
            "op-3",
            Some(77),
            9900
        ),
        "tiler-macos send native route=bridged phase=settle op=op-3 revision=Some(77) t_ms=900 now_ms=9900 membership=Some([277]) current=Some(277) changed=false",
    );
}

#[test]
fn process_clock_is_monotonic() {
    // The shared attribution clock never runs backwards across calls on
    // this thread (same origin as the eligibility store).
    let first = send_now_ms();
    let second = send_now_ms();
    assert!(second >= first);
    assert_eq!(send_now_ms(), crate::input_eligibility::now_ms());
}

#[test]
fn producer_path_emits_every_phase_with_both_clocks_and_no_new_unsafe() {
    // Phase attribution producer guard: the live backend emits the
    // topology/proof/reread/release/reconcile boundaries (drag) plus the
    // dispatch/select/arrival boundaries (fast bridged) on the
    // metadata-only lines with the shared clock, posts only through the
    // maintained HID wrappers, never touches the fixture post target, and
    // keeps the module unsafe ban.
    let source = include_str!("workspace_send_native.rs");
    for phase in [
        "topology",
        "proof",
        "reread",
        "release",
        "reconcile",
        "dispatch",
        "select",
        "arrival",
    ] {
        assert!(
            source.contains(&format!("\"{phase}\"")),
            "live backend must emit the {phase} phase boundary"
        );
    }
    for token in ["send_now_ms", "phase_op", "phase_revision"] {
        assert!(
            source.contains(token),
            "phase lines must join on op/revision plus the shared clock ({token})"
        );
    }
    assert!(
        source.contains("hid_mouse_post") && source.contains("hid_key_post"),
        "managed posts reuse the maintained ffi posting wrappers"
    );
    assert!(
        !source.contains("OwnedPostTarget::"),
        "managed product path must not touch the fixture post target"
    );
    assert!(
        source.contains("forbid(unsafe_code)") && !source.contains("unsafe "),
        "no new unsafe on the managed path"
    );
}
