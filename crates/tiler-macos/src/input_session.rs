#![forbid(unsafe_code)]

//! Bounded owned `input-session` fixture (Child-holder, input LIVE only).
//!
//! Exercises the REAL native tile input source on a dedicated input owner
//! thread ([`crate::native_input::TapTileFactory`]) against a known owned helper
//! window, with downstream delivery proof from the helper's own AppKit key
//! counters (exact vs extra-modifier vs plain passthrough), never just a
//! tap callback or API return. Synthetic posts ride native CG through the
//! signed LaunchServices controller ([`crate::synth`], truthful synthetic,
//! never physical); before EVERY individual post the selected oracle
//! ([`crate::focus::read_agreement`]) names the exact intended owned
//! target PID/WID immediately, and anything else refuses. Never a Ghostty
//! (or any foreign) event target. NO tiling or frame writes anywhere on
//! this path: focus moves only via the SAME SLPS route with agreement for
//! target and restore (shared with `focus-session`).
//!
//! Legs (all bounded, all with settle readbacks) over the tap route:
//! - tap: exact `Ctrl+h` suppressed (owner match plus helper absence over a
//!   bounded window), extra/plain passthrough, matched Left/Right arrow
//!   suppression only after the `h` proof AND a single-Space baseline (Up/
//!   Down never posted: no user-owned overview restore path exists),
//!   release post after exit.
//! - `--stall-once-ms` (tap only): the first exact-match KeyDown stalls
//!   ONCE inside the tap callback to provoke a REAL OS
//!   `TapDisabledByTimeout` (never a manual callback); the leg then proves
//!   the OS disable notification/counter, re-enable, a recovery follow-up
//!   post, and passthrough after disable/exit.
//! - `--physical-stall` (tap only with `--stall-once-ms`): the same stalled
//!   tap, but the exact leg awaits ONE physical Ctrl+h press (no synthetic
//!   exact post) so the stall consumes real HID load; synthetic stalls alone
//!   never proved a timeout (enabled-readback false is not timeout evidence).
//!   Recovery still re-proves suppression synthetically.
//! - `--direct-disable-hold-ms` (tap only): the owner parks the tap
//!   DIRECTLY OS-disabled (`CGEventTapEnable(false)`, fixture-only hold
//!   flag) while the main posts the exact chord; the leg proves the actual
//!   `CGEventTapIsEnabled==false` readback plus helper passthrough (+1/+1),
//!   then clears the hold, proves the `==true` recovery readback, and
//!   re-proves exact suppression. Labeled DIRECT disable/recovery throughout
//!   (never the OS-timeout notification path).
//! - `--direct-rearm` (tap only): EXPLICIT fresh re-arm after an intentional
//!   own-fault DIRECT disable. The owner parks the OLD tap directly
//!   OS-disabled while the main posts the exact chord (actual
//!   `==false` readback plus helper passthrough +1/+1, no owner hit), then
//!   the owner drops the old guard WITHOUT a same-tap re-enable and installs
//!   a FRESH `TapSource` over the same chords and tap placement on the SAME
//!   owner thread (epoch bump plus fresh `==true` readback), and the main
//!   re-proves exact suppression on the new generation (owner match plus
//!   helper absence plus fresh tap counters). Labeled DIRECT-REARM
//!   throughout (never the OS-timeout notification path, never a same-tap
//!   re-enable claim). A hard creation-denial refusal fails closed with the
//!   refusal (never masked).
//! - `--disable-once` (tap only): one-shot disable-once fault with no
//!   fixture hold and no assumed notification kind. The owner disables the
//!   tap directly once while it keeps pumping; the tap recovers automatically
//!   through whichever path the OS takes (measured live: a UserInput
//!   notification with a verified same-tap re-enable), then the main
//!   re-proves exact suppression. The leg reports the ACTUAL kind from the
//!   counters (`user-input` vs `unnotified-enabled-readback`) and never a
//!   timeout claim without the actual timeout counter. Labeled DISABLE-ONCE
//!   throughout with the actual kind named (never an explicit re-arm claim).
//! - `--watchdog-only` (tap only, 2..=5s): no helper, no posts, no completed
//!   JSON. The probe arms the REAL tap owner over the exact 34-chord
//!   catalogue, emits a `watchdog_ready` binding (PID plus run ID), parks
//!   the main past the shared hard watchdog, and the watchdog forces the
//!   dirty process exit. The parent harness holds the owned helper
//!   separately and proves release with an owned post after the exit.
//!
//! The hard lifetime is the shared [`crate::watchdog::Watchdog`] (finite
//! startup plus run plus teardown plus grace, dirty forced exit rather than
//! a hang); termination proves actual process quiescence (verified exit,
//! reap, no residue) plus input delivery after release. The watchdog-only
//! leg inverts this: the dirty exit IS the evidence (no completed JSON).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{MAX_PROBE_SECONDS, ProbeModifier};
use crate::ffi::TapSourceReport;
use crate::focus::{FocusAgreement, FocusDispatch, HolderStop, OriginalPreimage, SpaceSnapshot};
use crate::input::cg_flag_bits;
use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
use crate::input_session_legs::{arrow_observations, post_chord_leg, run_post_legs};
use crate::native_input::{TapTileFactory, tile_catalogue};
use crate::runtime_backend::{InputSource, InputSourceFactory};
use crate::synth::{GuardedPostRoute, PostedEvent, post_guarded};
use crate::watchdog::Watchdog;

/// Default session lifetime: spawn plus focus plus posts plus restore fits
/// comfortably; the watchdog is the hard bound either way.
pub const DEFAULT_INPUT_SECONDS: u64 = 60;
/// One-shot tap-callback stall bound (fault-injection leg only).
pub const MAX_STALL_ONCE_MS: u64 = 2000;
/// Physical-stall minimum lifetime (user reaction plus recovery legs).
pub const MIN_PHYSICAL_STALL_SECONDS: u64 = 45;
/// Physical-stall await bound for the single user press.
pub const PHYSICAL_STALL_AWAIT_SECS: u64 = 30;
/// Fixture-only DIRECT disable hold bound (fault-injection leg only):
/// the owner parks the tap directly OS-disabled while the hold is set.
pub const MAX_DIRECT_HOLD_MS: u64 = 1000;
/// Watchdog-only probe bounds (hard-exit leg only): the shared watchdog is
/// the run itself, so the whole probe fits in 2..=5s with no helper, no
/// posts, and no completed JSON (dirty exit is the expected evidence).
pub const MIN_WATCHDOG_ONLY_SECONDS: u64 = 2;
pub const MAX_WATCHDOG_ONLY_SECONDS: u64 = 5;
/// Probe keycode: Mac virtual keycode `0x04` (ANSI `h`, same as
/// [`crate::input::PROBE_KEY_CODE`]).
pub(crate) const KEY_H: u32 = 0x04;
/// `CGEventFlags` Shift bit (same mask space as [`cg_flag_bits`]).
pub(crate) const CG_SHIFT_BIT: u64 = 0x0002_0000;
/// `CGEventFlags` Alternate bit (same mask space as [`cg_flag_bits`]).
pub(crate) const CG_ALT_BIT: u64 = 0x0008_0000;
/// Device-independent modifier mask (Shift|Ctrl|Alt|Cmd, same space as
/// [`cg_flag_bits`]).
pub(crate) const MOD_MASK: u64 = 0x001E_0000;
/// Settle bounds: delivery wait per post, absence window for suppression
/// proof, helper snapshot poll slice.
pub(crate) const DELIVERY_WAIT: Duration = Duration::from_millis(3000);
pub(crate) const ABSENCE_WAIT: Duration = Duration::from_millis(1500);
pub(crate) const POLL_SLICE_MS: u64 = 50;

/// Which native tile source the dedicated owner drives. The tap is the sole
/// route (no auto fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InputSessionRoute {
    Tap,
}

impl InputSessionRoute {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "tap" => Ok(Self::Tap),
            other => Err(format!("unknown --route '{other}' (expected tap)")),
        }
    }
}

/// Which CG tap the synthetic producer posts at. `session` is the normal
/// default; `hid` is the fixture-only upstream discriminator (same approved
/// chords/owned window, no permission-tier change): local producer evidence
/// (`hammerspoon/extensions/eventtap/libeventtap.m` `keyStrokes` posts at
/// `kCGHIDEventTap` when no target app is given) shows HID as the upstream
/// injection point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PostRoute {
    Session,
    Hid,
}

impl PostRoute {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "session" => Ok(Self::Session),
            "hid" => Ok(Self::Hid),
            other => Err(format!(
                "unknown --post-route '{other}' (expected session|hid)"
            )),
        }
    }
}

/// Validated session request. Construction never touches the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSessionRequest {
    pub seconds: u64,
    pub route: InputSessionRoute,
    pub post_route: PostRoute,
    pub stall_once_ms: u64,
    pub physical_stall: bool,
    pub direct_disable_hold_ms: u64,
    pub direct_rearm: bool,
    pub disable_once: bool,
    pub watchdog_only: bool,
    pub modifier: ProbeModifier,
}

impl InputSessionRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.watchdog_only {
            if self.seconds < MIN_WATCHDOG_ONLY_SECONDS || self.seconds > MAX_WATCHDOG_ONLY_SECONDS
            {
                return Err(format!(
                    "watchdog-only seconds must be {MIN_WATCHDOG_ONLY_SECONDS}..={MAX_WATCHDOG_ONLY_SECONDS}, got {}",
                    self.seconds
                ));
            }
            if self.stall_once_ms > 0 {
                return Err(
                    "watchdog-only takes no --stall-once-ms (separate fault leg)".to_owned(),
                );
            }
            if self.direct_disable_hold_ms > 0 {
                return Err(
                    "watchdog-only takes no --direct-disable-hold-ms (separate fault leg)"
                        .to_owned(),
                );
            }
            if self.direct_rearm {
                return Err("watchdog-only takes no --direct-rearm (separate fault leg)".to_owned());
            }
            if self.disable_once {
                return Err("watchdog-only takes no --disable-once (separate fault leg)".to_owned());
            }
            if self.physical_stall {
                return Err(
                    "watchdog-only takes no --physical-stall (separate fault leg)".to_owned(),
                );
            }
            return Ok(());
        }
        if self.seconds == 0 || self.seconds > MAX_PROBE_SECONDS {
            return Err(format!(
                "seconds must be 1..={MAX_PROBE_SECONDS}, got {}",
                self.seconds
            ));
        }
        if self.stall_once_ms > MAX_STALL_ONCE_MS {
            return Err(format!(
                "stall-once-ms must be 1..={MAX_STALL_ONCE_MS}, got {}",
                self.stall_once_ms
            ));
        }
        if self.direct_disable_hold_ms > MAX_DIRECT_HOLD_MS {
            return Err(format!(
                "direct-disable-hold-ms must be 1..={MAX_DIRECT_HOLD_MS}, got {}",
                self.direct_disable_hold_ms
            ));
        }
        if self.direct_disable_hold_ms > 0 && self.stall_once_ms > 0 {
            return Err(
                "direct-disable-hold and stall-once are separate fault legs (never combined)"
                    .to_owned(),
            );
        }
        if self.direct_rearm {
            if self.stall_once_ms > 0 {
                return Err(
                    "direct-rearm and stall-once are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
            if self.direct_disable_hold_ms > 0 {
                return Err(
                    "direct-rearm and direct-disable-hold are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
        }
        if self.disable_once {
            if self.stall_once_ms > 0 {
                return Err(
                    "disable-once and stall-once are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
            if self.direct_disable_hold_ms > 0 {
                return Err(
                    "disable-once and direct-disable-hold are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
            if self.direct_rearm {
                return Err(
                    "disable-once and direct-rearm are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
        }
        if self.physical_stall {
            if self.stall_once_ms == 0 {
                return Err("physical-stall needs --stall-once-ms (the stalled tap)".to_owned());
            }
            if self.direct_disable_hold_ms > 0 {
                return Err(
                    "physical-stall and direct-disable-hold are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
            if self.direct_rearm {
                return Err(
                    "physical-stall and direct-rearm are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
            if self.disable_once {
                return Err(
                    "physical-stall and disable-once are separate fault legs (never combined)"
                        .to_owned(),
                );
            }
            if self.seconds < MIN_PHYSICAL_STALL_SECONDS {
                return Err(format!(
                    "physical-stall seconds must be >={MIN_PHYSICAL_STALL_SECONDS} (user reaction plus recovery), got {}",
                    self.seconds
                ));
            }
        }
        Ok(())
    }
}

/// Modifier nibble bucket for helper counters: `(flags & MOD_MASK) >> 17`
/// (bit0=Shift bit1=Ctrl bit2=Alt bit3=Cmd, 16 distinct buckets).
#[must_use]
pub fn bucket_index(flags: u64) -> usize {
    ((flags & MOD_MASK) >> 17) as usize
}

/// Exact-match tuning mirror for one catalogue chord (same construction as
/// the input owner: Ctrl-role bit plus the Shift arm, the Alt resize arm,
/// and the auxiliary workspace arm under the same mask).
pub(crate) fn tuning_for(
    modifier: ProbeModifier,
    key_code: u32,
    shift: bool,
    alt: bool,
    aux: bool,
) -> TapTuning {
    let (mut want_flags, flags_mask) = cg_flag_bits(modifier);
    if shift {
        want_flags |= CG_SHIFT_BIT;
    }
    if alt {
        want_flags |= CG_ALT_BIT;
    }
    if aux {
        want_flags |= crate::input_policy::aux_flag_for(modifier);
    }
    TapTuning {
        want_flags,
        flags_mask,
        want_code: i64::from(key_code),
    }
}

/// An extra masked modifier bit matched by NO catalogue chord (fails
/// exact match on every arm — focus, Shift move, Alt grow, Alt+Shift
/// shrink — by construction; pure, unit-tested). The Alt resize arm now
/// owns the Alt bit, so the extra rides Command under the Ctrl role and
/// Control everywhere else (each masked, each outside every armed chord for
/// its role; Shift would alias the move arm for the default Ctrl role —
/// measured live: the tap correctly matched index 8 — so Shift is never
/// the extra):
/// - Ctrl role: Cmd (`Ctrl+Cmd+H` matches nothing — Ctrl, Ctrl+Shift,
///   Ctrl+Alt, and Ctrl+Alt+Shift on H are all live chords);
/// - Meta role: Ctrl (`Cmd+Ctrl+H` matches nothing);
/// - Alt role: Ctrl (`Alt+Ctrl+H` matches nothing — the Alt-role resize
///   arm collapses onto focus/move, refused at validation);
/// - Shift role: Ctrl (`Shift+Ctrl+H` matches nothing — the Shift role is
///   refused at validation like before).
///
/// This is the product auxiliary-bit contract: the workspace catalogue
/// arms exactly this bit on its relative/previous/output chords (see
/// [`crate::input_policy::aux_flag_for`], the single home this delegates
/// to).
#[must_use]
pub fn extra_flag_for(modifier: ProbeModifier) -> u64 {
    crate::input_policy::aux_flag_for(modifier)
}

// -- Dedicated input owner ---------------------------------------------------

/// One exact owner-side match: catalogue index, edge, arrival time. The
/// session correlates these with its post times by order (posts are strictly
/// sequential with settle waits, so order is preserved).
#[derive(Debug, Clone, Copy)]
pub(crate) struct OwnerHit {
    pub(crate) index: usize,
    pub(crate) down: bool,
    pub(crate) at: Instant,
}

enum OwnerFactory {
    Tap(Arc<TapTileFactory>),
}

pub(crate) struct OwnerDone {
    pub(crate) unexpected_eos: u64,
    pub(crate) ignored: u64,
    pub(crate) matched: u64,
}

/// Dedicated input-owner thread body: creates, drives, and destroys its
/// REAL native tile source inside this thread. Exact chord
/// matches (indexed by catalogue order) are forwarded as [`OwnerHit`];
/// everything else counts as ignored. A `None` without stop is an honest
/// unexpected EOS (counted, thread exits, never spun on).
fn owner_main(
    factory: OwnerFactory,
    modifier: ProbeModifier,
    stop: Arc<AtomicBool>,
    hit_tx: mpsc::Sender<OwnerHit>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) -> OwnerDone {
    let create = match &factory {
        OwnerFactory::Tap(factory) => factory.create(),
    };
    let mut source: Box<dyn InputSource> = match create {
        Ok(source) => {
            let _ = ready_tx.send(Ok(()));
            source
        }
        Err(reason) => {
            let _ = ready_tx.send(Err(reason));
            return OwnerDone {
                unexpected_eos: 0,
                ignored: 0,
                matched: 0,
            };
        }
    };
    let catalogue = tile_catalogue();
    let tunings: Vec<(TapTuning, usize)> = catalogue
        .iter()
        .enumerate()
        .map(|(index, binding)| {
            (
                tuning_for(
                    modifier,
                    binding.key_code,
                    binding.shift,
                    binding.alt,
                    binding.aux,
                ),
                index,
            )
        })
        .collect();
    let mut done = OwnerDone {
        unexpected_eos: 0,
        ignored: 0,
        matched: 0,
    };
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Some(event) = source.next(&stop) else {
            if !stop.load(Ordering::SeqCst) {
                done.unexpected_eos += 1;
            }
            break;
        };
        let mut index = None;
        for (tuning, candidate) in &tunings {
            let mut pending = false;
            if decide_tap_event(
                event.kind,
                event.flags,
                i64::from(event.key_code),
                *tuning,
                false,
                &mut pending,
            )
            .matched
            {
                index = Some(*candidate);
                break;
            }
        }
        match index {
            Some(index) => {
                done.matched += 1;
                let _ = hit_tx.send(OwnerHit {
                    index,
                    down: event.kind == TapKeyKind::KeyDown,
                    at: Instant::now(),
                });
            }
            None => done.ignored += 1,
        }
    }
    done
}

// -- Session context ---------------------------------------------------------

/// Plain session metadata (no handles): the held Child travels separately
/// as `&mut raw.child` so revalidation and signalling always use the exact
/// held handle, never a stored PID.
pub(crate) struct CtxMeta {
    pub(crate) pid: u32,
    pub(crate) wid: u32,
    pub(crate) helper_path: String,
    pub(crate) modifier: ProbeModifier,
    pub(crate) want_flags: u64,
    pub(crate) post_route: PostRoute,
    pub(crate) deadline: Instant,
}

/// Gate ONE synthetic post through the shared guarded boundary (the normal
/// dispatch path: session taps observe it,
/// and the frontmost app — which the oracle must confirm is exactly
/// `(pid, wid)` RIGHT NOW — receives it unless a tap suppresses): Secure
/// Input must not be active, the session must read unlocked, and the time
/// budget must hold. Owned proof comes from the unforgeable
/// [`OwnedPostTarget`] (exact held `Child` plus executable plus CG join,
/// constructed per edge); the boundary re-enforces the fresh selected
/// oracle plus the Ghostty/protected refusal immediately before EACH post.
/// Never a Ghostty (or any foreign) target: only the held helper passes.
/// `meta.post_route` selects the CG producer tap: `session` (default) or
/// `hid` (fixture-only upstream discriminator).
pub(crate) fn gated_post(
    child: &mut std::process::Child,
    meta: &CtxMeta,
    key_code: u16,
    flags: u64,
    down: bool,
) -> Result<PostedEvent, String> {
    if Instant::now() >= meta.deadline {
        return Err("session time budget exhausted; refusing further posts".to_owned());
    }
    if crate::ffi::secure_input_known() == Some(true) {
        return Err("refusing post: Secure Input active".to_owned());
    }
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err("refusing post: session locked".to_owned());
        }
        crate::ffi::SessionLock::Unknown => {
            return Err("refusing post: session unknown".to_owned());
        }
    }
    let target = crate::synth::OwnedPostTarget::from_held_child(
        child,
        meta.pid,
        meta.wid,
        &meta.helper_path,
    )?;
    let route = match meta.post_route {
        PostRoute::Session => GuardedPostRoute::Session,
        PostRoute::Hid => GuardedPostRoute::Hid,
    };
    post_guarded(&target, key_code, flags, down, route)
}

/// Latest helper key-count snapshot: drain every pending stdout line from
/// the helper's own pipe (snapshots print every 500 ms) and keep the newest
/// `key_counts` object. Counts only, never titles or content.
pub(crate) fn latest_helper_counts(
    lines: &mpsc::Receiver<String>,
) -> Option<crate::ffi::KeyCounts> {
    let mut latest = None;
    while let Ok(line) = lines.try_recv() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
            && let Some(counts) = value.get("key_counts")
            && let Ok(parsed) = serde_json::from_value::<crate::ffi::KeyCounts>(counts.clone())
        {
            latest = Some(parsed);
        }
    }
    latest
}

/// Pump the controller main thread while waiting (Application events plus
/// the runloop in bounded slices): workspace reads
/// need a serviced main thread. No windows, no activation, no focus effect.
pub(crate) fn pump_main() {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        if let Some(mtm) = MainThreadMarker::new() {
            objc2::rc::autoreleasepool(|_| {
                let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
                crate::ffi::drain_app_events(&app);
                crate::ffi::pump_runloop_once(POLL_SLICE_MS);
            });
        } else {
            std::thread::sleep(Duration::from_millis(POLL_SLICE_MS));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // No AppKit here: bounded park only, same slice as the macOS pump.
        crate::ffi::pump_runloop_once(POLL_SLICE_MS);
        std::thread::sleep(Duration::from_millis(POLL_SLICE_MS));
    }
}

// -- Per-post leg evidence ---------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PostLeg {
    pub(crate) name: String,
    pub(crate) key_code: u32,
    pub(crate) flags: u64,
    pub(crate) down_post_us: u64,
    pub(crate) up_post_us: u64,
    /// Owner hit latency (down-post instant through owner arrival), when an
    /// owner hit was expected AND observed, in milliseconds.
    pub(crate) owner_latency_ms: Option<u64>,
    /// Helper counter deltas over the leg: (down, up) in the exact bucket.
    pub(crate) helper_exact_delta: (u64, u64),
    /// Helper counter deltas in the extra-modifier bucket.
    pub(crate) helper_extra_delta: (u64, u64),
    /// Helper counter deltas in the plain (no-modifier) bucket.
    pub(crate) helper_plain_delta: (u64, u64),
    /// Actual `CGEventTapIsEnabled` readbacks around the leg (`None` for
    /// non-DIRECT legs): the DIRECT disable/recovery proof gates on these,
    /// never on a callback atomic alone.
    pub(crate) direct_enabled_before: Option<bool>,
    pub(crate) direct_enabled_after: Option<bool>,
    pub(crate) verdict: String,
}

// -- Report ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ArrowStatus {
    pub(crate) key_code: u32,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    pub(crate) tap_observed: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct InputSessionReport {
    route: InputSessionRoute,
    post_route: PostRoute,
    modifier: String,
    seconds: u64,
    trust_observed: Option<bool>,
    post_event_access: Option<bool>,
    helper_pid: u32,
    helper_wid: u32,
    helper_bundle: String,
    focus_latency_ms: u64,
    restore_latency_ms: u64,
    tap_report: TapSourceReport,
    owner_matched: u64,
    owner_ignored: u64,
    owner_unexpected_eos: u64,
    legs: Vec<PostLeg>,
    stall_once_ms: u64,
    physical_stall: bool,
    direct_disable_hold_ms: u64,
    direct_rearm: bool,
    disable_once: bool,
    rearm_epoch: u64,
    watchdog_only: bool,
    arrows: Vec<ArrowStatus>,
    space_before: SpaceSnapshot,
    space_after: SpaceSnapshot,
    space_unchanged: bool,
    restore_dispatch: FocusDispatch,
    restore_agreement: FocusAgreement,
    post_stop_readback: Option<FocusAgreement>,
    stop: HolderStop,
    gated_ms: u64,
    input_refusal: Option<String>,
    outcome: String,
    note: String,
}

/// Watchdog-only hard-exit probe (tap only, 2..=5s, fixture-only): no helper,
/// no posts, no completed JSON. Arms the REAL tap owner over the exact
/// 34-chord catalogue, emits a `watchdog_ready` binding (PID plus run ID plus
/// executable) on stdout, parks the main past the shared hard watchdog, and
/// the watchdog forces the dirty process exit (`Process::exit(1)` with the
/// `hard lifetime exceeded` stderr line). The parent harness holds the owned
/// helper separately (existing `probe-spawn --regular-policy` holder),
/// focuses it, starts this probe via LaunchServices, binds the exact PID
/// (plus start/exe/run ID), observes the PID gone within the bound plus
/// process quiescence, then proves release with an owned post. A `completed`
/// JSON here would mean the watchdog failed to fire (returned, never
/// resolve success from a return).
fn run_watchdog_only_session(request: InputSessionRequest) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error, "watchdog_only": true}).to_string(),
        )
    };
    if !crate::ffi::slps_available() {
        return Err(err_json(
            "slps-unavailable",
            "SLPS focus symbols unavailable; stop — no SA, no policy change".to_owned(),
        ));
    }
    let trust = crate::ffi::ax_is_process_trusted();
    if trust != Some(true) {
        return Err(err_json(
            "ax-trust",
            format!("AX trust not observed true ({trust:?}); stop — user consent needed"),
        ));
    }
    if crate::ffi::secure_input_known() == Some(true) {
        return Err(err_json(
            "secure-input",
            "Secure Input active; stop — interception blackout respected".to_owned(),
        ));
    }
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err(err_json("session", "session locked; stop".to_owned()));
        }
        crate::ffi::SessionLock::Unknown => {
            return Err(err_json("session", "session unknown; stop".to_owned()));
        }
    }
    // Hard lifetime IS the run: armed BEFORE the tap effect with NO disarm
    // on the success path (the dirty exit is the evidence). `Drop` disarms
    // only on the early-error returns below.
    let mut watchdog =
        Watchdog::arm(Duration::from_secs(request.seconds)).map_err(|reason| (1, reason))?;
    // Exact 34-chord catalogue over the REAL tap source on a dedicated owner
    // thread (same owner body as the full session; no posts flow here).
    let catalogue = tile_catalogue();
    debug_assert_eq!(catalogue.len(), 34);
    let factory = Arc::new(
        TapTileFactory::new(request.modifier, catalogue)
            .map_err(|e| err_json("input-factory", e))?,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let (hit_tx, hit_rx) = mpsc::channel::<OwnerHit>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let owner_factory = OwnerFactory::Tap(Arc::clone(&factory));
    let owner_stop = Arc::clone(&stop);
    let owner_modifier = request.modifier;
    let owner_handle = std::thread::Builder::new()
        .name("input-owner".to_owned())
        .spawn(move || owner_main(owner_factory, owner_modifier, owner_stop, hit_tx, ready_tx))
        .map_err(|e| err_json("input-owner", format!("owner spawn: {e}")))?;
    // Owner readiness (bounded well under the watchdog: the tap registered
    // or refused while the main thread stays pumped). A timeout here is an
    // honest error return (watchdog disarms via `Drop`); the success path
    // below never returns.
    let ready_deadline = Instant::now() + Duration::from_secs(request.seconds);
    let ready_outcome = loop {
        match ready_rx.try_recv() {
            Ok(result) => break result,
            Err(mpsc::TryRecvError::Disconnected) => {
                stop.store(true, Ordering::SeqCst);
                let _ = owner_handle.join();
                watchdog.disarm();
                return Err(err_json(
                    "input-ready",
                    "owner exited before readiness".to_owned(),
                ));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if Instant::now() >= ready_deadline {
            stop.store(true, Ordering::SeqCst);
            let _ = owner_handle.join();
            watchdog.disarm();
            return Err(err_json(
                "input-ready",
                "owner readiness timed out".to_owned(),
            ));
        }
        pump_main();
    };
    if let Err(reason) = ready_outcome {
        stop.store(true, Ordering::SeqCst);
        let _ = owner_handle.join();
        watchdog.disarm();
        return Err(err_json(
            "input-ready",
            format!("input source refused: {reason}"),
        ));
    }
    // Drain any stray owner hits (no posts flow, so none are expected; the
    // receiver stays drained so the parked main holds no backlog).
    while hit_rx.try_recv().is_ok() {}
    // Readiness binding BEFORE the park: exact PID plus per-run ID plus
    // executable, so an independent controller binds (PID + start/exe) and
    // addresses exactly this process. No user content: IDs only.
    let runid = crate::probe_target::mint_nonce()
        .unwrap_or_else(|_| format!("run-{}-{}", std::process::id(), request.seconds));
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| String::from("<unknown-exe>"));
    println!(
        "{}",
        serde_json::json!({
            "watchdog_ready": true,
            "watchdog_only": true,
            "pid": std::process::id(),
            "runid": runid,
            "exe": exe,
            "route": "tap",
            "catalogue_chords": 34,
            "watchdog_secs": request.seconds,
        })
    );
    {
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
    }
    // Deliberate park past the hard watchdog: the main sleeps while the
    // owner pumps the REAL tap (fast callbacks, no blocking) and the
    // watchdog fires independently (dirty exit, never a return). The sleep
    // bound is intentionally far past the watchdog so the watchdog is the
    // only exit path; reaching the code after it means the watchdog failed.
    std::thread::sleep(Duration::from_secs(request.seconds.saturating_add(30)));
    // Watchdog failed to fire: disarm, stop the owner, and fail loudly
    // (never claim the hard exit that did not happen).
    stop.store(true, Ordering::SeqCst);
    let _ = owner_handle.join();
    watchdog.disarm();
    Err(err_json(
        "watchdog-did-not-fire",
        "parked past the hard watchdog but the process did not exit; refusing to claim a hard exit"
            .to_owned(),
    ))
}

/// Bounded owned input session. See module docs for the legs. Exit codes:
/// 2 usage/validation, 1 runtime leg failure or refusal with evidence.
pub fn run_input_session(request: InputSessionRequest) -> Result<String, (i32, String)> {
    request.validate().map_err(|e| (2, e))?;
    if request.watchdog_only {
        return run_watchdog_only_session(request);
    }
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    if !crate::ffi::slps_available() {
        return Err(err_json(
            "slps-unavailable",
            "SLPS focus symbols unavailable; stop — no SA, no policy change".to_owned(),
        ));
    }
    let trust = crate::ffi::ax_is_process_trusted();
    if trust != Some(true) {
        return Err(err_json(
            "ax-trust",
            format!("AX trust not observed true ({trust:?}); stop — user consent needed"),
        ));
    }
    if crate::ffi::secure_input_known() == Some(true) {
        return Err(err_json(
            "secure-input",
            "Secure Input active; stop — interception/posting blackout respected".to_owned(),
        ));
    }
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err(err_json("session", "session locked; stop".to_owned()));
        }
        crate::ffi::SessionLock::Unknown => {
            return Err(err_json("session", "session unknown; stop".to_owned()));
        }
    }
    match crate::ffi::cg_post_event_access_known() {
        Some(true) => {}
        access => {
            return Err(err_json(
                "post-access",
                format!(
                    "window server post access not granted ({access:?}); stop — posts would drop"
                ),
            ));
        }
    }
    // Hard lifetime BEFORE any effect: soft run plus teardown plus grace,
    // then a dirty forced exit. `Drop` disarms as a backstop.
    let mut watchdog =
        Watchdog::arm(Duration::from_secs(request.seconds) + Duration::from_secs(60))
            .map_err(|reason| (1, reason))?;
    let deadline = Instant::now() + Duration::from_secs(request.seconds);

    // Original preimage + Space baseline BEFORE the spawn (same discipline
    // as focus-session: the regular-policy fixture may take frontmost at
    // launch, which must never invalidate the foreign preimage).
    let original: OriginalPreimage = {
        let retry_deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match crate::focus::capture_original(None) {
                Ok(o) => break o,
                Err(e) => {
                    if Instant::now() >= retry_deadline {
                        return Err(err_json("preimage", e.to_string()));
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    };
    let space_before = crate::focus::capture_space().map_err(|e| err_json("space-before", e))?;
    let single_space_baseline = space_before
        .displays
        .iter()
        .map(|d| d.spaces.len())
        .sum::<usize>()
        <= 1;

    // Owned helper spawn: regular policy prefers the dedicated signed
    // helper `.app` (stable helper bundle ID with a LaunchServices entry).
    let nonce = crate::probe_target::mint_nonce().map_err(|e| err_json("spawn", e))?;
    let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, request.seconds)
        .map(|s| s.with_regular_policy(true))
        .map_err(|e| err_json("spawn", e))?;
    let mut raw =
        crate::probe_target::start_helper_process(&spec).map_err(|e| err_json("spawn", e))?;
    let pid = raw.pid;
    let helper_path = raw.helper_path.clone();
    if let Err(e) = crate::probe_target::await_handshake(&mut raw, &spec) {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "handshake", "error": e, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    if let Err(e) = crate::probe_target::record_spawn(pid, &spec, &helper_path) {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "record", "error": e, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    let helper_bundle = crate::ownership::verify_live_process(pid, Some(&helper_path)).map_err(|e| {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        (
            1,
            serde_json::json!({"stage": "ownership", "error": e, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        )
    })?;
    if crate::ownership::is_protected_bundle(&helper_bundle) {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "ownership", "error": format!("helper resolved to protected bundle '{helper_bundle}'; refusing"), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    let wid = match crate::focus::join_helper_wid(pid) {
        Ok(w) => w,
        Err(e) => {
            let restore_attempt = crate::focus::attempt_restore(&original);
            let (space_after, stop, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "cg-join", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
    };
    if crate::cg::verify_wid_owned_by_pid(pid, wid).is_err() {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "cg-join", "error": "wid not joined to owned pid", "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    let space_joined = crate::focus::capture_space().map_err(|e| {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        (
            1,
            serde_json::json!({"stage": "space-joined", "error": e, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        )
    })?;
    if space_joined != space_before {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "space-joined", "error": "spawn moved Spaces; refusing", "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    if let Err(e) =
        crate::focus::recheck_holder_before_mutation(&mut raw.child, pid, &helper_path, wid)
    {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "pre-mutation", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    // Non-vacuous proof (same discipline as focus-session): when the spawn
    // itself already holds full agreement, restore the original FIRST so
    // the measured dispatch starts proven-unfocused.
    if crate::focus::read_agreement(pid, wid).is_ok() {
        if let Err(e) = crate::focus::verify_original_currency(&original) {
            let (space_after, stop, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "pre-restore-currency", "error": format!("{e} (FOCUS NOT RESTORED)"), "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
        if crate::focus::dispatch_focus(original.pid as u32, original.wid).is_err() {
            let (space_after, stop, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "pre-restore-dispatch", "error": "pre-restore dispatch failed (FOCUS NOT RESTORED)", "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
        if crate::focus::await_agreement(original.pid as u32, original.wid, Duration::from_secs(5))
            .is_err()
        {
            let (space_after, stop, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "pre-restore-agreement", "error": "PRE-RESTORE FAILED — STOP", "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
        if let Err(e) =
            crate::focus::recheck_holder_before_mutation(&mut raw.child, pid, &helper_path, wid)
        {
            let (space_after, stop, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "pre-mutation-recheck", "error": e.to_string(), "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
    }
    // Measured focus dispatch to the owned window with agreement.
    let t_focus = Instant::now();
    if let Err(e) = crate::focus::dispatch_focus(pid, wid) {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "focus-dispatch", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    if let Err(e) = crate::focus::await_agreement(pid, wid, Duration::from_secs(5)) {
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "focus-agreement", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    let focus_latency_ms = crate::focus::latency_ms(t_focus);

    // Dedicated input owner over the REAL native tile source (tap only).
    // Fixture-only explicit unrestricted test gate for known post
    // targets (strict per-edge oracle still enforced in `gated_post`
    // via the shared guarded boundary); normal tile stays scoped.
    let catalogue = tile_catalogue();
    let (want_flags, _) = cg_flag_bits(request.modifier);
    let stop = Arc::new(AtomicBool::new(false));
    let (hit_tx, hit_rx) = mpsc::channel::<OwnerHit>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let tap_factory = Arc::new(
        TapTileFactory::new(request.modifier, catalogue)
            .map(|f| {
                let f = f.with_unrestricted_test_gate();
                if request.stall_once_ms > 0 {
                    f.with_stall_once(request.stall_once_ms)
                } else {
                    f
                }
            })
            .map_err(|e| err_json("input-factory", e))?,
    );
    let owner_factory = OwnerFactory::Tap(Arc::clone(&tap_factory));
    let owner_stop = Arc::clone(&stop);
    let owner_handle = std::thread::Builder::new()
        .name("input-owner".to_owned())
        .spawn(move || {
            owner_main(
                owner_factory,
                request.modifier,
                owner_stop,
                hit_tx,
                ready_tx,
            )
        })
        .map_err(|e| err_json("input-owner", format!("owner spawn: {e}")))?;
    // Owner readiness (bounded): the source registered (or refused) within
    // the remaining budget while the main thread stays pumped.
    let ready_statuses = {
        let ready_deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match ready_rx.try_recv() {
                Ok(result) => break result,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = stop_and_cleanup_owner(stop, owner_handle);
                    let restore_attempt = crate::focus::attempt_restore(&original);
                    let (space_after, stop_info, post_stop_readback) =
                        crate::focus::stop_and_readback(&mut raw, &original);
                    return Err((
                        1,
                        serde_json::json!({"stage": "input-ready", "error": "owner exited before readiness", "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
                    ));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if Instant::now() >= ready_deadline || Instant::now() >= deadline {
                let _ = stop_and_cleanup_owner(stop, owner_handle);
                let restore_attempt = crate::focus::attempt_restore(&original);
                let (space_after, stop_info, post_stop_readback) =
                    crate::focus::stop_and_readback(&mut raw, &original);
                return Err((
                    1,
                    serde_json::json!({"stage": "input-ready", "error": "owner readiness timed out", "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
                ));
            }
            pump_main();
        }
    };
    if let Err(reason) = ready_statuses {
        let _ = stop_and_cleanup_owner(stop, owner_handle);
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop_info, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "input-ready", "error": format!("input source refused: {reason}"), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }

    // Post-readiness focus re-assertion (causal sequencing repair): source
    // creation plus the readiness wait pump the main AFTER the initial helper
    // focus agreement, and focus can drift back to the original frontmost in
    // that window (measured live: Ghostty agreement at the first post with
    // rearm epoch 0, no post ever flowing). Reassert the intended helper
    // focus HERE — after the owner is ready, before any post leg — with the
    // exact held Child plus executable plus CG recheck, the SAME SLPS
    // dispatch route, and a fresh selected-oracle agreement await (which pumps
    // the main while polling). Per-edge `gated_post` oracles stay unchanged
    // and still refuse anything but the exact target; a failed re-assertion
    // stops with data and never forces a post.
    if let Err(e) =
        crate::focus::recheck_holder_before_mutation(&mut raw.child, pid, &helper_path, wid)
    {
        let _ = stop_and_cleanup_owner(stop, owner_handle);
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop_info, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "focus-rerecheck", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    if let Err(e) = crate::focus::dispatch_focus(pid, wid) {
        let _ = stop_and_cleanup_owner(stop, owner_handle);
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop_info, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "focus-redispatch", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    if let Err(e) = crate::focus::await_agreement(pid, wid, Duration::from_secs(5)) {
        let _ = stop_and_cleanup_owner(stop, owner_handle);
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop_info, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "focus-reagreement", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }

    let meta = CtxMeta {
        pid,
        wid,
        helper_path: helper_path.clone(),
        modifier: request.modifier,
        want_flags,
        post_route: request.post_route,
        deadline,
    };
    let mut legs: Vec<PostLeg> = Vec::new();
    // Disjoint field borrows of the held spawn: the Child for
    // revalidation/signalling, the lines pipe for snapshot reads.
    let leg_result = run_post_legs(
        &mut raw.child,
        &meta,
        &raw.lines,
        &hit_rx,
        request,
        &space_before,
        single_space_baseline,
        &mut legs,
        Some(&*tap_factory),
    );
    if let Err((stage, cause)) = leg_result {
        let (owner_done, owner_joined) = stop_and_cleanup_owner(stop, owner_handle);
        let helper_last = latest_helper_counts(&raw.lines);
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop_info, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": stage, "error": cause, "legs": legs, "helper_last": helper_last, "owner_done": {"matched": owner_done.matched, "ignored": owner_done.ignored, "unexpected_eos": owner_done.unexpected_eos, "joined": owner_joined}, "tap_report": tap_factory.latest_report(), "tap_refusal": tap_factory.refusal(), "tap_direct_enabled": tap_factory.latest_direct_enabled(), "rearm_epoch": tap_factory.rearm_epoch(), "direct_rearm": request.direct_rearm, "disable_once": request.disable_once, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }

    // Termination first, release proof second: stop the owner (bounded
    // join; the source drops on thread exit, releasing tap interception
    // via RAII), then the release post must still reach
    // the helper with no owner hit.
    let (owner_done, _) = stop_and_cleanup_owner(stop, owner_handle);
    let mut release_leg = post_chord_leg(
        &mut raw.child,
        &meta,
        &raw.lines,
        &hit_rx,
        "release-exact",
        KEY_H,
        want_flags,
        None,
        false,
    );
    if let Ok(leg) = release_leg.as_mut() {
        leg.verdict = "posted-after-release".to_owned();
    }
    let release_leg = match release_leg {
        Ok(leg) => leg,
        Err(reason) => {
            let helper_last = latest_helper_counts(&raw.lines);
            let restore_attempt = crate::focus::attempt_restore(&original);
            let (space_after, stop_info, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "release-post", "error": reason, "legs": legs, "helper_last": helper_last, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
    };
    // The release leg must show delivery with no interception: helper
    // exact bucket +1/+1. (Owner hits are impossible: the owner is joined.)
    if release_leg.helper_exact_delta != (1, 1) {
        let helper_last = latest_helper_counts(&raw.lines);
        let restore_attempt = crate::focus::attempt_restore(&original);
        let (space_after, stop_info, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "release-post", "error": format!("release delivery mismatch (want exact +1/+1, have {:?})", release_leg.helper_exact_delta), "legs": legs, "release": release_leg, "helper_last": helper_last, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    legs.push(release_leg);

    // SAME-route restore with agreement, then Space readback and verified
    // exact-Child cleanup.
    if let Err(e) = crate::focus::verify_original_currency(&original) {
        let (space_after, stop_info, post_stop_readback) =
            crate::focus::stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "restore-currency", "error": format!("{e} (FOCUS NOT RESTORED)"), "legs": legs, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    let t_restore = Instant::now();
    let restore_dispatch = match crate::focus::dispatch_focus(original.pid as u32, original.wid) {
        Ok(d) => d,
        Err(e) => {
            let (space_after, stop_info, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "restore-dispatch", "error": e.to_string(), "legs": legs, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
    };
    let restore_agreement = match crate::focus::await_agreement(
        original.pid as u32,
        original.wid,
        Duration::from_secs(5),
    ) {
        Ok((agreement, _)) => agreement,
        Err(e) => {
            let (space_after, stop_info, post_stop_readback) =
                crate::focus::stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "restore-agreement", "error": e.to_string(), "legs": legs, "restore_dispatch": restore_dispatch, "space_before": space_before, "space_after": space_after, "stop": stop_info, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
    };
    let restore_latency_ms = crate::focus::latency_ms(t_restore);
    let space_after = crate::focus::capture_space().unwrap_or(SpaceSnapshot { displays: vec![] });
    let space_unchanged = space_after == space_before;
    let stop_info = crate::focus::stop_verified(&mut raw.child, raw.pid);
    let post_stop_readback = crate::focus::read_agreement(original.pid as u32, original.wid).ok();

    let tap_report = tap_factory.latest_report();
    let gated_ms = tap_factory.gated_ms();
    let input_refusal = tap_factory.refusal();
    let arrows = arrow_observations(&legs);
    watchdog.disarm();
    let outcome = if input_refusal.is_some() {
        "input-refused"
    } else {
        "completed"
    };
    let report = InputSessionReport {
        route: request.route,
        post_route: request.post_route,
        modifier: request.modifier.as_str().to_owned(),
        seconds: request.seconds,
        trust_observed: trust,
        post_event_access: crate::ffi::cg_post_event_access_known(),
        helper_pid: pid,
        helper_wid: wid,
        helper_bundle,
        focus_latency_ms,
        restore_latency_ms,
        tap_report,
        owner_matched: owner_done.matched,
        owner_ignored: owner_done.ignored,
        owner_unexpected_eos: owner_done.unexpected_eos,
        legs,
        stall_once_ms: request.stall_once_ms,
        physical_stall: request.physical_stall,
        direct_disable_hold_ms: request.direct_disable_hold_ms,
        direct_rearm: request.direct_rearm,
        disable_once: request.disable_once,
        rearm_epoch: tap_factory.rearm_epoch(),
        watchdog_only: request.watchdog_only,
        arrows,
        space_before,
        space_after,
        space_unchanged,
        restore_dispatch,
        restore_agreement,
        post_stop_readback,
        stop: stop_info,
        gated_ms,
        input_refusal,
        outcome: outcome.to_owned(),
        note: "input LIVE only: no tiling, no frame writes; stop leaves windows in place"
            .to_owned(),
    };
    let body = serde_json::to_string(&report).map_err(|e| (1, e.to_string()))?;
    if outcome == "completed" {
        Ok(body)
    } else {
        Err((1, body))
    }
}

/// Stop the owner thread (bounded join): the input source drops on thread
/// exit, releasing tap interception via RAII. Every
/// owner pump slice is bounded well under the join bound, so the join
/// cannot hang past it short of an OS-level wedge (which the watchdog
/// covers as the hard backstop).
/// Stop the owner thread (bounded join): returns the owner counters plus
/// whether the thread joined cleanly (`false` = the owner panicked instead
/// of returning its counters; the panic payload is not retrievable across
/// the thread boundary, so callers report the flag, never a guess).
fn stop_and_cleanup_owner(
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<OwnerDone>,
) -> (OwnerDone, bool) {
    stop.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    match handle.join() {
        Ok(done) => (done, true),
        Err(_) => (
            OwnerDone {
                unexpected_eos: 0,
                ignored: 0,
                matched: 0,
            },
            false,
        ),
    }
}
