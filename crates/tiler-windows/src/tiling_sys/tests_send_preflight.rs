#![forbid(unsafe_code)]
use super::send_preflight::SendPreflight;
use crate::native_guards::SendFenceSnapshot;

fn preflight(maximized: bool, fullscreen: bool, fallback: bool) -> SendPreflight {
    SendPreflight {
        fences: SendFenceSnapshot {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: false,
            overlay_maximized: maximized,
            overlay_fullscreen: fullscreen,
        },
        fallback_fullscreen: fallback,
    }
}

#[test]
fn overlay_hold_covers_pre_transfer_gate_without_touching_windows() {
    // Null handle reads (false, None) with no setters and no crash. This
    // is the exact production-called gate the native pre-assign check
    // runs before `assign` (mirroring the Engine pre-assign order), so an
    // observed mover that went borderless fullscreen refuses with NO
    // membership change; the gate/assign ordering itself is straight-line
    // code in `workspace_do_send_native`, not a mock.
    let fulls = [tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }];
    // Ordinary snapshot, unreadable frame, no fallback: holds.
    assert_eq!(
        preflight(false, false, false).overlay_hold(0, &fulls),
        Ok(())
    );
    // Unreadable frame with a fullscreen fallback refuses (the retained
    // native-boundary case): no transfer, no hide, no focus.
    assert_eq!(
        preflight(true, false, true).overlay_hold(0, &fulls),
        Err("send-refused-fullscreen")
    );
    // Ordinary snapshot with a fullscreen fallback refuses the newly
    // arrived fullscreen the same way.
    assert_eq!(
        preflight(false, false, true).overlay_hold(0, &fulls),
        Err("send-refused-fullscreen")
    );
    // Retained snapshot against a null live read defers drift with no
    // restore.
    assert_eq!(
        preflight(true, false, false).overlay_hold(0, &fulls),
        Err("deferred")
    );
    // Item 9: a fullscreen snapshot with an unreadable live flag defers
    // pre-effect instead of trusting the fallback (readable stable
    // fullscreen still carries through the portable gate, pinned there).
    assert_eq!(
        preflight(false, true, true).overlay_hold(0, &fulls),
        Err("deferred")
    );
    assert_eq!(
        preflight(false, true, false).overlay_hold(0, &fulls),
        Err("deferred")
    );
}
