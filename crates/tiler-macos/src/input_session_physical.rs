#![forbid(unsafe_code)]

//! Physical-stall exact leg for `input-session` (tap + `--stall-once-ms` +
//! `--physical-stall` only; split from [`crate::input_session_legs`] to keep
//! the fault-injection growth out of that file).
//!
//! The stalled tap awaits ONE physical Ctrl+h press (default Ctrl role)
//! instead of a synthetic exact post, so the one-shot callback stall
//! consumes real HID load. The stalled down must never reach the helper;
//! the disable verdict itself comes from the re-enable counter in the stall
//! stage, and the synthetic recovery leg re-proves suppression after it. A
//! missing press fails truthfully (never a synthetic substitute). Timeout
//! injection never reaches the product tile path: only the `input-session`
//! fixture arms the stall.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::config::ProbeModifier;
use crate::ffi::TapSourceReport;
use crate::input_session::{
    CtxMeta, OwnerHit, PHYSICAL_STALL_AWAIT_SECS, PostLeg, bucket_index, extra_flag_for, pump_main,
};
use crate::input_session_legs::{delta, hold_absence_allow_up, poll_helper_counts};
use crate::native_input::TapTileFactory;

/// User-facing press-now prompt. Pure (no OS touch) so its contract is
/// unit-pinned: exactly ONE press, the Ctrl-role modifier RELEASED for the
/// load taps (a held Ctrl would re-match the exact chord and fail the
/// suppression hold), and plain non-`h` letter keys for that load.
#[must_use]
pub(crate) fn physical_prompt(modifier: ProbeModifier) -> String {
    format!(
        "tiler-macos physical-stall-exact: press {}+h ONCE now (owned helper is frontmost; do not click), then RELEASE {} and tap plain letter keys (a / j / k, never h) for load; waiting {}s",
        modifier.as_str(),
        modifier.as_str(),
        PHYSICAL_STALL_AWAIT_SECS,
    )
}

/// Await the single physical exact press on the stalled tap. See module
/// docs for the contract.
pub(crate) fn await_physical_stall_leg(
    meta: &CtxMeta,
    lines: &mpsc::Receiver<String>,
    hits: &mpsc::Receiver<OwnerHit>,
    tap_factory: Option<&TapTileFactory>,
    h_index: usize,
    want: u64,
) -> Result<PostLeg, String> {
    const NAME: &str = "physical-stall-exact";
    while hits.try_recv().is_ok() {}
    let tap_before = tap_factory
        .map(|f| f.latest_report())
        .unwrap_or(TapSourceReport {
            matched: 0,
            suppressed: 0,
            deliveries: 0,
            reenables_after_timeout: 0,
            user_disables: 0,
            unnotified_disables: 0,
        });
    let before = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot before await"))?;
    // stderr (unbuffered): the LaunchServices wrapper streams it live with
    // `--stream`, so the prompt is visible BEFORE the await below expires.
    eprintln!("{}", physical_prompt(meta.modifier));
    let t_down = Instant::now();
    let wait = Duration::from_secs(PHYSICAL_STALL_AWAIT_SECS);
    let hit = loop {
        match hits.try_recv() {
            Ok(hit) => break hit,
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(format!("{NAME}: owner gone while awaiting physical press"));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if t_down.elapsed() >= wait {
            return Err(format!(
                "{NAME}: no physical {}+h owner hit within {}s; refusing a synthetic substitute",
                meta.modifier.as_str(),
                PHYSICAL_STALL_AWAIT_SECS,
            ));
        }
        if Instant::now() >= meta.deadline {
            return Err(format!(
                "{NAME}: session budget exhausted awaiting physical press"
            ));
        }
        pump_main();
    };
    if hit.index != h_index || !hit.down {
        return Err(format!(
            "{NAME}: owner hit mismatch (want index {h_index} down, have index {} down={})",
            hit.index, hit.down
        ));
    }
    hold_absence_allow_up(hits, NAME, h_index)?;
    let after = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot after press"))?;
    let tap_after = tap_factory
        .map(|f| f.latest_report())
        .unwrap_or(TapSourceReport {
            matched: 0,
            suppressed: 0,
            deliveries: 0,
            reenables_after_timeout: 0,
            user_disables: 0,
            unnotified_disables: 0,
        });
    let want_bucket = bucket_index(meta.want_flags);
    let extra_bucket = bucket_index(meta.want_flags | extra_flag_for(meta.modifier));
    let plain_bucket = bucket_index(0);
    let mut leg = PostLeg {
        name: NAME.to_owned(),
        key_code: crate::input_session::KEY_H,
        flags: want,
        down_post_us: 0,
        up_post_us: 0,
        owner_latency_ms: Some(
            hit.at
                .saturating_duration_since(t_down)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        ),
        helper_exact_delta: (
            delta(before.h_down[want_bucket], after.h_down[want_bucket]),
            delta(before.h_up[want_bucket], after.h_up[want_bucket]),
        ),
        helper_extra_delta: (
            delta(before.h_down[extra_bucket], after.h_down[extra_bucket]),
            delta(before.h_up[extra_bucket], after.h_up[extra_bucket]),
        ),
        helper_plain_delta: (
            delta(before.h_down[plain_bucket], after.h_down[plain_bucket]),
            delta(before.h_up[plain_bucket], after.h_up[plain_bucket]),
        ),
        direct_enabled_before: None,
        direct_enabled_after: None,
        verdict: "posted".to_owned(),
    };
    if leg.helper_exact_delta.0 != 0 {
        return Err(format!(
            "physical stalled down escaped suppression (helper exact {:?})",
            leg.helper_exact_delta
        ));
    }
    if leg.helper_extra_delta != (0, 0) {
        return Err(format!(
            "physical stall leg touched the extra bucket (extra {:?})",
            leg.helper_extra_delta
        ));
    }
    leg.verdict = format!(
        "physical-stall-consumed down-suppressed up-bypassed={} plain-delta={:?} tap matched+{} suppressed+{} reenables={} await_ms={}",
        leg.helper_exact_delta.1,
        leg.helper_plain_delta,
        tap_after.matched.saturating_sub(tap_before.matched),
        tap_after.suppressed.saturating_sub(tap_before.suppressed),
        tap_after.reenables_after_timeout,
        leg.owner_latency_ms.unwrap_or(u64::MAX)
    );
    Ok(leg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_pins_single_press_released_modifier_and_plain_keys() {
        // Behavior contract: the user must press exactly once, release the
        // Ctrl-role modifier for load, and use plain non-h keys (a held
        // modifier or a repeated h would re-match and fail the leg).
        for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
            let prompt = physical_prompt(modifier);
            assert!(prompt.contains("ONCE"), "prompt must demand one press");
            assert!(
                prompt.contains("RELEASE"),
                "prompt must demand modifier release"
            );
            assert!(
                prompt.contains("never h"),
                "prompt must forbid h for load taps"
            );
            assert!(
                prompt.contains(modifier.as_str()),
                "prompt must name the configured modifier"
            );
            assert!(
                prompt.contains(&PHYSICAL_STALL_AWAIT_SECS.to_string()),
                "prompt must state the await bound"
            );
            assert!(
                !prompt.contains("hold"),
                "prompt must never ask the user to hold a chord"
            );
        }
        // Default request modifier is ctrl (the documented user command
        // presses Ctrl+h with no --ctrl flag).
        assert_eq!(ProbeModifier::default(), ProbeModifier::Ctrl);
        assert!(physical_prompt(ProbeModifier::Ctrl).contains("ctrl+h"));
    }
}
