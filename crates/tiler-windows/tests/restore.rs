#![forbid(unsafe_code)]
use tiler_windows::model::{
    ObservedWindow, ProcessIdentity, RefuseReason, RestoreDecision, WindowClaimKind,
    WindowIdentity, restore_eligibility,
};

fn process() -> ProcessIdentity {
    ProcessIdentity {
        pid: 200,
        process_creation: "c-200".to_owned(),
        user_sid: "S-owner".to_owned(),
        session_id: 1,
        exe_path: "C:\\apps\\a.exe".to_owned(),
    }
}

fn record() -> WindowIdentity {
    WindowIdentity {
        hwnd: 0xABCD,
        process: process(),
        tag: "tag-1".to_owned(),
        kind: WindowClaimKind::Helper,
        show: tiler_windows::model::WindowShowState::default(),
    }
}

fn hidden() -> ObservedWindow {
    ObservedWindow {
        identity: record(),
        visible: false,
    }
}

#[test]
fn reveal() {
    assert_eq!(
        restore_eligibility(&record(), &[hidden()]),
        RestoreDecision::Reveal { hwnd: 0xABCD }
    );
}

#[test]
fn skip() {
    let observed = ObservedWindow {
        visible: true,
        ..hidden()
    };
    assert_eq!(
        restore_eligibility(&record(), &[observed]),
        RestoreDecision::SkipAlreadyVisible { hwnd: 0xABCD }
    );
}

#[test]
fn missing() {
    assert_eq!(
        restore_eligibility(&record(), &[]),
        RestoreDecision::Refuse {
            reason: RefuseReason::MissingObservation
        }
    );
}

#[test]
fn ambiguous() {
    assert_eq!(
        restore_eligibility(&record(), &[hidden(), hidden()]),
        RestoreDecision::Refuse {
            reason: RefuseReason::AmbiguousObservation
        }
    );
}

#[test]
fn process_mismatch() {
    let observed = ObservedWindow {
        identity: WindowIdentity {
            process: ProcessIdentity {
                process_creation: "c-999".to_owned(),
                ..process()
            },
            ..record()
        },
        visible: false,
    };
    assert_eq!(
        restore_eligibility(&record(), &[observed]),
        RestoreDecision::Refuse {
            reason: RefuseReason::ProcessMismatch
        }
    );
}

#[test]
fn tag_mismatch() {
    let observed = ObservedWindow {
        identity: WindowIdentity {
            tag: "tag-2".to_owned(),
            ..record()
        },
        visible: false,
    };
    assert_eq!(
        restore_eligibility(&record(), &[observed]),
        RestoreDecision::Refuse {
            reason: RefuseReason::TagMismatch
        }
    );
}
