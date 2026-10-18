//! Native send trace-line tests: the metadata-only leg/event/readback
//! lines carry exact IDs, geometry, hex chords, and elapsed timing — never
//! titles, content, or paths. No live touch: every line is pure over
//! explicit inputs.

use crate::space_drag_transfer::DragStep;
use crate::workspace_send_native::{
    format_native_key_line, format_native_leg_begin_line, format_native_mouse_line,
    format_native_readback_line, native_step_name,
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
fn leg_begin_pins_goal_grab_frame_and_relative_offset() {
    assert_eq!(
        format_native_leg_begin_line(277, (500.0, 400.0), frame()),
        "tiler-macos send native leg-begin goal=277 grab=(500,400) frame=(100,200,800,600) rel=(400,200)",
    );
}

#[test]
fn mouse_line_carries_elapsed_point_and_frame_relative_offset() {
    assert_eq!(
        format_native_mouse_line("mouse-down", 12, (500.0, 400.0), Some(frame())),
        "tiler-macos send native ev=mouse-down t_ms=12 pt=(500,400) rel=(400,200)",
    );
    // Frameless posts still trace the point (never a guessed relative).
    assert_eq!(
        format_native_mouse_line("mouse-move", 3, (500.0, 400.0), None),
        "tiler-macos send native ev=mouse-move t_ms=3 pt=(500,400)",
    );
}

#[test]
fn key_line_carries_elapsed_hex_chord() {
    assert_eq!(
        format_native_key_line("chord-down", 63, 0x13, 0x0004_0000),
        "tiler-macos send native ev=chord-down t_ms=63 key=0x13 flags=0x40000",
    );
    assert_eq!(
        format_native_key_line("key-up", 470, 0x13, 0x0004_0000),
        "tiler-macos send native ev=key-up t_ms=470 key=0x13 flags=0x40000",
    );
}

#[test]
fn readback_line_distinguishes_baseline_from_change() {
    assert_eq!(
        format_native_readback_line(900, &Some(vec![1, 277]), &Some(1), false),
        "tiler-macos send native readback t_ms=900 membership=Some([1, 277]) current=Some(1) changed=false",
    );
    assert_eq!(
        format_native_readback_line(1100, &Some(vec![277]), &Some(277), true),
        "tiler-macos send native readback t_ms=1100 membership=Some([277]) current=Some(277) changed=true",
    );
    // Unreadable legs render as None (unknown, never guessed).
    assert_eq!(
        format_native_readback_line(3000, &None, &None, true),
        "tiler-macos send native readback t_ms=3000 membership=None current=None changed=true",
    );
}
