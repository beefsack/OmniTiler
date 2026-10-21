#![forbid(unsafe_code)]
use super::*;

/// END-bound Esc latch decision: a physical Esc down cancels the gesture
/// only when the observed sequence differs from the START snapshot. Held
/// gestures observe the live counter; just-ended gestures observe the
/// END-callback snapshot, so an Esc that lands after the END (before the
/// owner drain) observes an equal snapshot and never cancels the completed
/// drop. Pure sequence comparison (wrapping-aware by construction: the
/// counter only moves forward one step per physical Esc down).
#[must_use]
pub(super) fn esc_edge_cancels(start_seq: u64, observed_seq: u64) -> bool {
    observed_seq != start_seq
}

/// Shared START capture for the native gesture producer (WinEvent
/// MOVESIZESTART): the Esc-sequence snapshot, the PRE-gesture rectangle,
/// and the START-time identity snapshot. PRE comes from the last stable
/// observation, never from a mid-gesture frame; live `GetWindowRect` is the
/// fallback only when `stable` has no entry. Probe failure leaves no
/// snapshot and settle falls back to the stored-member check only.
pub(super) fn capture_gesture_start(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    hwnd: u64,
    start_seq: u64,
) {
    // Sequence-bound Esc coordination: the snapshot rode with the START
    // event from callback time instead of being re-read here. Only edges
    // with a newer sequence latch later, so a stale pre-gesture tap never
    // cancels and a same-batch edge is ordered rather than discarded.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_esc_seq.entry(hwnd) {
        e.insert(start_seq);
    }
    if !state.gesture_before.contains_key(&hwnd) {
        let pre = state
            .stable
            .get(&hwnd)
            .copied()
            .or_else(|| live_outer_rect(hwnd).or_else(|| state.stable.get(&hwnd).copied()));
        if let Some(pre) = pre {
            state.gesture_before.insert(hwnd, pre);
        }
    }
    // START-time identity snapshot for the settle-time reuse fence: a
    // same-process HWND reuse between START and END fails closed even when
    // HWND/PID/creation still agree.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_start_key.entry(hwnd) {
        let probe = hwnd as isize as HWND;
        if let Some(live) = window_identity(probe, hwnd, me) {
            e.insert(crate::native_guards::WindowKey {
                hwnd,
                pid: live.pid,
                creation: live.process_creation.clone(),
            });
            // Member lifetime tag, not the owned-helper tag (empty for
            // ordinary windows): the settle fence compares this against a
            // fresh member read, exactly like the admission gate.
            state
                .gesture_start_tag
                .insert(hwnd, crate::product_hide::sys::read_member_tag(hwnd));
        }
    }
    // START-frozen preview source binding (first START wins, never
    // rebound): the exact member token, source output/workspace, and
    // accepted revision the preview samples and the final drop resolve
    // against. Unmanaged STARTs store nothing and fail closed in the
    // refresh; drift after START fails the whole gesture closed.
    if !state.gesture_preview_start.contains_key(&hwnd) {
        let start_binding = state
            .member_tokens
            .iter()
            .find(|(key, _)| key.hwnd == hwnd)
            .and_then(|(key, token)| {
                state.member_home(key).map(|loc| {
                    let domain_fp = state.preview_domain_fp(&loc.output);
                    PreviewStartBinding {
                        token: token.clone(),
                        output: loc.output,
                        workspace: loc.workspace,
                        domain_fp,
                    }
                })
            });
        match start_binding {
            Some(binding) => {
                state.gesture_preview_start.insert(hwnd, binding);
            }
            None => {
                state.gesture_preview_start.remove(&hwnd);
            }
        }
    }
}
