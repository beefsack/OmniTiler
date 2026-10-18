#![forbid(unsafe_code)]
use super::*;
use crate::input_handoff::{global_esc_clock, global_esc_edge};
use std::sync::atomic::Ordering;

fn fresh_state() {
    test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
}

#[test]
fn own_e8_reentry_passes_through_with_no_state_change() {
    // The mask `SendInput` pair reenters this callback on the same
    // thread as injected E8 with `dwExtraInfo` 0: product filters ALL
    // injected, so the reentry passes through with no borrow held,
    // no queue push, no sequence bump, and no panic.
    fresh_state();
    let tls_before = SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| (st.queue.len(), st.queue.dropped, st.esc_seq))
            .unwrap_or((0, 0, 0))
    });
    test_drive_llproc(super::super::VK_MASK, 0, LLKHF_INJECTED, 0);
    test_drive_llproc(super::super::VK_MASK, 0, LLKHF_INJECTED | LLKHF_UP, 0);
    // Thread-local state untouched (process-global clocks are shared
    // with parallel tests, so only TLS effects assert exactly here).
    let tls_after = SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| (st.queue.len(), st.queue.dropped, st.esc_seq))
            .unwrap_or((0, 0, 0))
    });
    assert_eq!(tls_before, tls_after);
    // The reentry is trace-visible (closed E8 vocabulary) but never
    // classifier state: enabling trace records exactly the two
    // filtered summaries and nothing else.
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.cb_diag_enabled = true;
        }
    });
    test_drive_llproc(super::super::VK_MASK, 0, LLKHF_INJECTED, 0);
    let diags = drain_callback_diag();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].reason, CallbackReason::InjectedFiltered);
    assert!(!diags[0].consumed);
    test_clear_state();
}

#[test]
fn physical_esc_sets_edge_and_bumps_clock() {
    // Serial guard: the global edge assertions race a concurrent
    // pump `ClearEsc` apply (which stores false).
    let _guard = crate::input_handoff::fence_test_serial();
    fresh_state();
    let esc_before = global_esc_clock().load();
    test_drive_llproc(super::super::VK_ESCAPE, 1, 0, 0);
    let edge = SNAP.with(|s| s.borrow().as_ref().map(|st| st.esc_edge).unwrap_or(false));
    assert!(edge);
    assert!(global_esc_clock().load() >= esc_before.wrapping_add(1));
    assert!(global_esc_edge().load(Ordering::SeqCst));
    test_clear_state();
}

#[test]
fn injected_esc_sets_no_edge() {
    // Synthetic Esc (unmarked, like a `SendInput` cancel): never a
    // command, never the native edge, passes through untouched.
    // Serial guard: same global-edge race as the physical Esc test above.
    let _guard = crate::input_handoff::fence_test_serial();
    fresh_state();
    test_drive_llproc(super::super::VK_ESCAPE, 1, LLKHF_INJECTED, 0);
    let (edge, esc_seq_delta) = SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| (st.esc_edge, st.esc_seq))
            .unwrap_or((true, 1))
    });
    assert!(!edge);
    assert_eq!(esc_seq_delta, 0);
    test_clear_state();
}

#[test]
fn untracked_win_up_passes_without_mask_send() {
    // A Win-up with no tracked hold reserves nothing, sends nothing
    // (no real `SendInput`), and always passes through.
    fresh_state();
    let (sends_before, _) = mask_send_stats();
    test_drive_llproc(super::super::VK_LWIN, 0, LLKHF_UP, 0);
    let (sends_after, _) = mask_send_stats();
    assert_eq!(sends_before, sends_after);
    test_clear_state();
}

#[test]
fn publication_fence_refuses_fresh_chord_through_real_callback() {
    // Real policy through the test-drive seam: while the fence is
    // pending (policy published, pump not yet applied) a fresh chord
    // passes through instead of consuming under stale policy; after
    // the pump applies, fresh chords consume, and a consumed hold
    // rides its stored verdict (repeat, paired Up, mask obligation)
    // across a later fence window. No `SendInput` here: Win-up is
    // never driven (the mask reserve is asserted directly).
    use crate::input_handoff::{
        fence_test_allocate, fence_test_serial, keyboard_fresh_gate, note_policy_applied,
        note_policy_published,
    };
    let _guard = fence_test_serial();
    let base = fence_test_allocate();
    fresh_state();
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.active = true;
        }
    });
    test_drive_llproc(super::super::VK_LWIN, 0, 0, 0);
    assert!(SNAP.with(|s| { s.borrow().as_ref().is_some_and(|st| st.machine.win_held()) }));
    // Fence pending: the fresh H down passes through the real
    // callback (never swallowed under stale policy).
    note_policy_published(base + 1);
    assert!(!keyboard_fresh_gate(true));
    let verdict = test_drive_llproc(super::super::VK_H, 0, 0, 0);
    assert_ne!(verdict, 1);
    assert!(SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|st| st.machine.key_is_down(super::super::VK_H))
    }));
    // Pump applies: a new fresh chord consumes through the callback.
    note_policy_applied(base + 1);
    assert!(keyboard_fresh_gate(true));
    let verdict = test_drive_llproc(super::super::VK_J, 0, 0, 0);
    assert_eq!(verdict, 1);
    // A newer policy publishes mid-hold: the repeat still rides the
    // stored consumed verdict, the mask obligation survives, and the
    // paired Up stays swallowed; the passed H pair still closes
    // passing (never stolen).
    note_policy_published(base + 2);
    assert!(!keyboard_fresh_gate(true));
    let verdict = test_drive_llproc(super::super::VK_J, 0, 0, 0);
    assert_eq!(verdict, 1);
    let mask_armed = SNAP.with(|s| {
        let mut borrow = s.borrow_mut();
        let st = borrow.as_mut().expect("hook state");
        super::super::win_up_mask_reserve(
            &mut st.machine,
            &mut st.queue,
            super::super::VK_LWIN,
            true,
            std::time::Instant::now(),
        )
    });
    assert!(mask_armed);
    let verdict = test_drive_llproc(super::super::VK_J, 0, LLKHF_UP, 0);
    assert_eq!(verdict, 1);
    let verdict = test_drive_llproc(super::super::VK_H, 0, LLKHF_UP, 0);
    assert_ne!(verdict, 1);
    note_policy_applied(base + 2);
    assert!(keyboard_fresh_gate(true));
    test_clear_state();
}

#[test]
fn proof_marker_matrix_accepts_only_the_exact_marker() {
    assert!(!super::super::accept_proof_injected(0, None));
    assert!(!super::super::accept_proof_injected(
        super::super::SHORTCUT_PROOF_MARKER,
        None
    ));
    assert!(!super::super::accept_proof_injected(
        0,
        Some(super::super::SHORTCUT_PROOF_MARKER)
    ));
    assert!(super::super::accept_proof_injected(
        super::super::SHORTCUT_PROOF_MARKER,
        Some(super::super::SHORTCUT_PROOF_MARKER)
    ));
}
