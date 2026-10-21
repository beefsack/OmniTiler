#![forbid(unsafe_code)]
use super::is_limited_fullscreen;
use super::settings::geometry_write_allowed;
use crate::native_guards::{ForegroundFacts, ForegroundVeto, ForegroundVetoReason};

fn veto_read(block: bool, reason: ForegroundVetoReason) -> ForegroundVeto {
    ForegroundVeto { block, reason }
}

fn facts_covering() -> ForegroundFacts {
    ForegroundFacts {
        valid: true,
        is_desktop: false,
        visible: true,
        captioned: false,
        dwm_readable: true,
        cloak_readable: true,
        cloaked: false,
        covers_monitor: true,
    }
}

#[test]
fn limited_policy_excludes_only_foreground() {
    use crate::native_guards::classify_foreground;
    let covering = facts_covering();
    let veto = classify_foreground(covering);
    assert!(veto.block && veto.reason == ForegroundVetoReason::Fullscreen);
    let read = super::settings::ForegroundRead {
        veto,
        desktop: false,
        visible: true,
        captioned: false,
        dwm_readable: true,
        cloaked: false,
    };
    assert!(is_limited_fullscreen(&read));
    // Foreground itself never writes; another window may.
    assert!(!geometry_write_allowed(&read, 0x9911, 0x9911));
    assert!(geometry_write_allowed(&read, 0x1001, 0x9911));
    // Unknown foreground fails closed: nothing writes.
    assert!(!geometry_write_allowed(&read, 0x1001, 0));
    // Other blocking shapes stay fully suspended.
    for reason in [
        ForegroundVetoReason::Invalid,
        ForegroundVetoReason::Unreadable,
    ] {
        let blocked = veto_read(true, reason);
        let other = super::settings::ForegroundRead {
            veto: blocked,
            desktop: false,
            visible: true,
            captioned: false,
            dwm_readable: true,
            cloaked: false,
        };
        assert!(
            !is_limited_fullscreen(&other),
            "{reason:?} must not be limited"
        );
        assert!(!geometry_write_allowed(&other, 0x1001, 0x9911));
        assert!(!geometry_write_allowed(&other, 0x9911, 0x9911));
    }
    // Non-blocking shapes always allow through revalidation.
    for reason in [
        ForegroundVetoReason::None,
        ForegroundVetoReason::ManagedOverlay,
        ForegroundVetoReason::Desktop,
    ] {
        let open = veto_read(false, reason);
        let other = super::settings::ForegroundRead {
            veto: open,
            desktop: false,
            visible: true,
            captioned: false,
            dwm_readable: true,
            cloaked: false,
        };
        assert!(!is_limited_fullscreen(&other));
        assert!(geometry_write_allowed(&other, 0x1001, 0x9911));
    }
}
