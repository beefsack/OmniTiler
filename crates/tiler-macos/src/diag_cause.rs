#![forbid(unsafe_code)]

//! Minimal opt-in cause diagnostics for the TextEdit hold/focus/close-delay
//! investigation (offline, portable, headless-testable).
//!
//! Everything here is gated on [`enabled`] (`TILER_MACOS_STAGE_TIMING=1`):
//! disabled costs one cached flag load and emits nothing. No behavior
//! change: no new native queries, no safety weakening, no write/read
//! changes. All identifiers are opaque numerics, all outcomes are fixed
//! static tokens; titles, user content, bundle strings, and raw error text
//! never reach any sink.
//!
//! Two sinks, one clock ([`crate::stage_timing::now_us`] on every line that
//! carries a stamp):
//! - Engine/worker threads (which hold `Shared`) emit through the existing
//!   bounded async ring via [`emit_diag`]: `tiler-log` lines with a `diag-`
//!   kind. `ref` carries the join identifier documented per kind (dispatch
//!   seq, observation seq, or `cmd_id`); `count` carries one small explicit
//!   number (never content). The ring cannot carry strings, so per-domain
//!   scope identity rides an index into the reply's own `scopes` order
//!   (joined via `ref`); exact observed window identity rides the existing
//!   staged pre/post rows, and only present/known/equals-target bits ride
//!   the count.
//! - Owner-thread frame code (no `Shared` access) prints `tiler-diag`
//!   stderr lines via [`emit_frame_diag`], the same pattern as the existing
//!   `frame-set` line. These carry explicit `key=value` fields
//!   (pid/wid/revision/entry/completion/counts/per-leg cause) because the
//!   whole transaction report plus completion is in scope at the call site.
//!
//! Schema:
//! - `diag-focus-<cause>` (`explicit|admission|follows|close`, or `close`
//!   for envelope exits without a request): `outcome` is the exact route
//!   predicate token below, `pid`/`gen` are the target app identity (0 when
//!   unparsed/unknown), `ref` is the dispatch seq, `count` packs the
//!   retained observed-focus evidence bits 0=present, 1=known,
//!   2=equals-target (read-only retained/shared facts, never a fresh read).
//! - `diag-report` outcome `fg-known-target|fg-known-windowless|fg-unknown`,
//!   `count` = unreadable native-domain count, `ref` = observation seq.
//! - `diag-report-view` outcome `complete|incomplete`, `count` =
//!   skipped-output count, `ref` = observation seq.
//! - `diag-report-roster` outcome `complete` (count = app count) or
//!   `incomplete-<cause>` per incomplete app (`pid` = app, `count` = that
//!   app's roster size). Cause tokens pass only the exact values
//!   constructed in this crate, else `unknown`.
//! - `diag-reply` outcome = per-scope authority
//!   (`controlled|untouched|held-<reason>`), `count` = scope index into the
//!   applied reply's `scopes` order, `ref` = dispatch seq. One line per
//!   scope: held AND clear are both explicit.
//! - `diag-close` outcome = pipeline stage (`report-sent|reply-applied`),
//!   `ref` = stage join id (observation seq, dispatch seq), `count` =
//!   `now_us` on the common monotonic clock.
//! - stderr `stage=frame`: `tiler-diag stage=frame stamp_us=<n> pid=<p>
//!   wid=<w> rev=<expect_revision> entry=<none|degenerate|entry-fence>
//!   completion=<applied|partial|failed|timeout> attempted=<n> applied=<n>
//!   readbacks=<n> unreadable=<n> leg0=<class[:cause]> leg1=... leg2=...`
//!   Leg classes: `applied`, `refused:<cause>`, `safety:<cause>`,
//!   `redundant`. Attempted setters are legs that issued an AX write
//!   (applied or refused); entry exits and safety-skipped legs issue none.
//!   Readbacks are bounded per-leg re-reads; only withheld legs skip.
//!
//! [`diag_cause`] is the diagnostic-only refusal classifier: every safety
//! predicate keeps an exact token (the runtime sanitizer would collapse
//! several of these into `failed-set`/`failed-snapshot`/`failed-backend`),
//! and anything unmatched falls back to
//! [`crate::runtime_log::sanitize_backend_reason`] (which preserves the
//! `kAXError` allowlist). Unknown strings never pass through.

use std::sync::{Arc, Mutex};

/// Ring kinds for cause diagnostics. Every kind carries the `diag-` prefix
/// so one filter selects the whole schema.
pub mod kind {
    /// Focus route decision, per request cause (plus envelope exits).
    pub const FOCUS_EXPLICIT: &str = "diag-focus-explicit";
    /// Focus route decision, automatic admission cause.
    pub const FOCUS_ADMISSION: &str = "diag-focus-admission";
    /// Focus route decision, focus-follows-cursor cause.
    pub const FOCUS_FOLLOWS: &str = "diag-focus-follows";
    /// Focus route decision, positive-close cause.
    pub const FOCUS_CLOSE: &str = "diag-focus-close";
    /// Report foreground provenance plus unreadable-domain count.
    pub const REPORT: &str = "diag-report";
    /// Report inventory completeness plus skipped-output count.
    pub const REPORT_VIEW: &str = "diag-report-view";
    /// Roster completeness (one line) or per-app incomplete cause.
    pub const REPORT_ROSTER: &str = "diag-report-roster";
    /// Per-scope applied-reply authority (held and clear both explicit).
    pub const REPLY: &str = "diag-reply";
    /// Close-pipeline stage with a common-clock stamp.
    pub const CLOSE: &str = "diag-close";
}

/// Close-pipeline stage outcomes for `diag-close` (`ref` is the stage join
/// id, `count` is `now_us` on the common monotonic clock).
pub mod close_stage {
    /// Engine dispatch: report built, about to transact. `ref` =
    /// observation seq.
    pub const REPORT_SENT: &str = "report-sent";
    /// Engine dispatch: reply accepted and consumed. `ref` = dispatch seq.
    pub const REPLY_APPLIED: &str = "reply-applied";
}

/// Whether cause diagnostics are enabled. Same switch as stage timing
/// (`TILER_MACOS_STAGE_TIMING=1`); off by default.
#[must_use]
pub fn enabled() -> bool {
    crate::stage_timing::enabled()
}

/// Emit one diagnostic ring record. Disabled: one flag load, no formatting,
/// no emission.
pub(crate) fn emit_diag(
    shared: &Arc<Mutex<crate::runtime_shared::Shared>>,
    pid: u32,
    generation: u64,
    ref_id: u64,
    kind: &'static str,
    outcome: &'static str,
    count: u64,
) {
    if !enabled() {
        return;
    }
    crate::runtime_shared::emit(shared, pid, generation, ref_id, kind, outcome, count);
}

/// Ring kind for one focus request cause.
#[must_use]
pub const fn focus_cause_kind(cause: tiler_core::world_report::FocusRequestCause) -> &'static str {
    use tiler_core::world_report::FocusRequestCause;
    match cause {
        FocusRequestCause::ExplicitCommand => kind::FOCUS_EXPLICIT,
        FocusRequestCause::AutomaticAdmission => kind::FOCUS_ADMISSION,
        FocusRequestCause::FocusFollowsCursor => kind::FOCUS_FOLLOWS,
        FocusRequestCause::PositiveClose => kind::FOCUS_CLOSE,
    }
}

/// Static token for one decoded [`VisibilityReason`](tiler_core::world_report::VisibilityReason).
/// `Empty` maps to `empty` (the wire form is the empty string, unusable as
/// a log outcome).
#[must_use]
pub const fn reason_token(reason: tiler_core::world_report::VisibilityReason) -> &'static str {
    use tiler_core::world_report::VisibilityReason;
    match reason {
        VisibilityReason::Empty => "empty",
        VisibilityReason::IdentityStale => "identity-stale",
        VisibilityReason::ScopeHeld => "scope-held",
        VisibilityReason::Unreadable => "unreadable",
        VisibilityReason::Unsupported => "unsupported",
        VisibilityReason::PreimageUnavailable => "preimage-unavailable",
        VisibilityReason::ForegroundChanged => "foreground-changed",
        VisibilityReason::Fullscreen => "fullscreen",
        VisibilityReason::Game => "game",
        VisibilityReason::ShowBlocked => "show-blocked",
        VisibilityReason::FocusUnproven => "focus-unproven",
        VisibilityReason::Superseded => "superseded",
        VisibilityReason::NativeRefused => "native-refused",
    }
}

/// Static outcome for a deferred-selection bind refusal carrying `reason`.
/// Every variant is enumerated so the exact predicate survives without any
/// per-reply allocation.
#[must_use]
pub const fn deferred_refusal_outcome(
    reason: tiler_core::world_report::VisibilityReason,
) -> &'static str {
    use tiler_core::world_report::VisibilityReason;
    match reason {
        VisibilityReason::Empty => "deferred-refused-empty",
        VisibilityReason::IdentityStale => "deferred-refused-identity-stale",
        VisibilityReason::ScopeHeld => "deferred-refused-scope-held",
        VisibilityReason::Unreadable => "deferred-refused-unreadable",
        VisibilityReason::Unsupported => "deferred-refused-unsupported",
        VisibilityReason::PreimageUnavailable => "deferred-refused-preimage-unavailable",
        VisibilityReason::ForegroundChanged => "deferred-refused-foreground-changed",
        VisibilityReason::Fullscreen => "deferred-refused-fullscreen",
        VisibilityReason::Game => "deferred-refused-game",
        VisibilityReason::ShowBlocked => "deferred-refused-show-blocked",
        VisibilityReason::FocusUnproven => "deferred-refused-focus-unproven",
        VisibilityReason::Superseded => "deferred-refused-superseded",
        VisibilityReason::NativeRefused => "deferred-refused-native-refused",
    }
}

/// Static token for one roster `roster_cause` value. Only the exact values
/// constructed in this crate map through; anything else (including a
/// hypothetical future cause) is `unknown`, never passed through.
#[must_use]
pub fn roster_cause_token(cause: &str) -> &'static str {
    match cause {
        "cg-alive" => "cg-alive",
        "cg-unreadable" => "cg-unreadable",
        "no-context" => "no-context",
        "stale-revision" => "stale-revision",
        "blank-hidden" => "blank-hidden",
        "unknown-no-frame" => "unknown-no-frame",
        "unknown-output-ungrounded" => "unknown-output-ungrounded",
        "classifier-omitted-minimized" => "classifier-omitted-minimized",
        "classifier-omitted-hidden-app" => "classifier-omitted-hidden-app",
        "" => "complete",
        _ => "unknown",
    }
}

/// Diagnostic-only refusal classifier: every safety predicate in the
/// scoped frame/focus fences keeps an exact static token (the runtime
/// sanitizer would collapse several of these into `failed-set`,
/// `failed-snapshot`, or `failed-backend`). Anything unmatched falls back
/// to [`crate::runtime_log::sanitize_backend_reason`], which preserves the
/// `kAXError` allowlist. Unknown strings never pass through.
#[must_use]
pub fn diag_cause(reason: &str) -> &'static str {
    // Ordered most-specific first: several coarse substrings below are
    // prefixes of finer predicates.
    const RULES: &[(&str, &str)] = &[
        ("Secure Input active", "secure-input-active"),
        ("loginwindow frontmost", "session-loginwindow-locked"),
        ("locked session (window server)", "session-server-locked"),
        ("session unknown", "session-unknown"),
        ("refusing window id 0", "wid-zero"),
        ("worker stopped mid-plan", "worker-stopped"),
        ("superseded by a newer desired set", "frame-superseded"),
        ("stale command pid", "stale-pid"),
        ("pid mismatch (stale command)", "stale-pid"),
        ("Ghostty at the effect boundary", "scope-ghostty"),
        (
            "own helper bundle at the effect boundary",
            "scope-helper-bundle",
        ),
        ("own bundle at the effect boundary", "scope-own-bundle"),
        ("out of scope: refusing write", "scope-out-of-scope"),
        ("unknown bundle: refusing write", "scope-unknown-bundle"),
        (
            "refusing own helper-bundle identity",
            "identity-helper-bundle",
        ),
        ("refusing own-bundle identity", "identity-own-bundle"),
        ("refusing own-process identity", "identity-own-process"),
        ("refusing Ghostty identity", "identity-ghostty"),
        ("out of scope: refusing owner", "identity-out-of-scope"),
        ("unknown bundle: refusing owner", "identity-unknown-bundle"),
        ("identity launch date unreadable", "identity-no-launch-date"),
        ("without an executable path", "identity-no-exe"),
        ("without a launch date", "identity-no-launch-date"),
        ("identity drift", "identity-drift"),
        ("out-of-range pid", "pid-out-of-range"),
        ("fence target mismatch", "fence-target-mismatch"),
        ("CG window list unreadable", "cg-unreadable"),
        ("wid absent from CG list", "cg-absent"),
        ("wid owned by another pid", "cg-owner-mismatch"),
        ("Space membership unreadable", "membership-unreadable"),
        (
            "not exactly on the current user Space",
            "membership-off-current-space",
        ),
        ("not exactly on one Space", "membership-multi-space"),
        ("homed by != 1 domain", "space-home-ambiguous"),
        ("planned domain drifted", "domain-drift"),
        ("display rows unreadable", "display-rows-unreadable"),
        ("display UUID unknown", "display-uuid-unknown"),
        ("drifted mid-plan", "space-drift"),
        ("no longer user-typed", "space-non-user"),
        ("no native context published", "ctx-cleared"),
        ("native context cleared", "ctx-cleared"),
        ("no effect hook bound", "no-hook"),
        ("topology revision drift", "revision-drift"),
        ("revision drift; superseded", "revision-drift"),
        ("minimized: withholding", "minimized-withheld"),
        ("hidden app: withholding", "hidden-app-withheld"),
        ("fullscreen overlay: withholding", "fullscreen-withheld"),
        ("fullscreen unreadable", "fullscreen-unreadable"),
        ("minimized unreadable", "minimized-unreadable"),
        ("refusing whole snapshot", "snapshot-incomplete"),
        ("subscription sync", "observer-sync-failed"),
        ("window identity join failed", "identity-join-failed"),
        ("no AX element for pid", "no-ax-element"),
        ("messaging timeout unset", "messaging-timeout-unset"),
        ("messaging timeout", "messaging-timeout"),
        ("not joined to owned", "wid-not-joined"),
        ("wid not joined", "wid-not-joined"),
        ("degenerate frame", "degenerate-frame"),
        ("frame actuation is macOS-only", "off-macos-frame"),
        ("focus dispatch unsupported", "focus-unsupported-backend"),
    ];
    for (needle, token) in RULES {
        if reason.contains(needle) {
            return token;
        }
    }
    crate::runtime_log::sanitize_backend_reason(reason)
}

/// One focus-route diagnostic record. `kind` selects the request cause
/// (see [`focus_cause_kind`]); `outcome` is the exact route predicate
/// token at the exit. `count` packs retained observed-focus evidence bits
/// 0=present, 1=known, 2=equals-target. Disabled: one flag load.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_focus_diag(
    shared: &Arc<Mutex<crate::runtime_shared::Shared>>,
    target_pid: u32,
    app_generation: u64,
    seq: u64,
    kind: &'static str,
    outcome: &'static str,
    observed_present: bool,
    observed_known: bool,
    observed_is_target: bool,
) {
    if !enabled() {
        return;
    }
    let count = (u64::from(observed_present))
        | (u64::from(observed_known) << 1)
        | (u64::from(observed_is_target) << 2);
    emit_diag(
        shared,
        target_pid,
        app_generation,
        seq,
        kind,
        outcome,
        count,
    );
}

/// Report diagnostics at dispatch: foreground provenance, inventory
/// completeness, and per-app roster causes, all from existing
/// retained/native facts. `apps` is `(pid, incomplete, cause, roster_len)`
/// per retained app. Disabled: one flag load.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_report_diag(
    shared: &Arc<Mutex<crate::runtime_shared::Shared>>,
    pid: u32,
    generation: u64,
    obs_seq: u64,
    focused_empty: bool,
    foreground_known: bool,
    inventory_complete: bool,
    unreadable_domains: u64,
    skipped_outputs: u64,
    apps: &[(u32, bool, &str, u64)],
) {
    if !enabled() {
        return;
    }
    let foreground = if foreground_known {
        if focused_empty {
            "fg-known-windowless"
        } else {
            "fg-known-target"
        }
    } else {
        "fg-unknown"
    };
    emit_diag(
        shared,
        pid,
        generation,
        obs_seq,
        kind::REPORT,
        foreground,
        unreadable_domains,
    );
    emit_diag(
        shared,
        pid,
        generation,
        obs_seq,
        kind::REPORT_VIEW,
        if inventory_complete {
            "complete"
        } else {
            "incomplete"
        },
        skipped_outputs,
    );
    let mut any_incomplete = false;
    for (app_pid, incomplete, cause, roster_len) in apps {
        if *incomplete {
            any_incomplete = true;
            let token = roster_cause_token(cause);
            let outcome: &'static str = match token {
                "cg-alive" => "incomplete-cg-alive",
                "cg-unreadable" => "incomplete-cg-unreadable",
                "no-context" => "incomplete-no-context",
                "stale-revision" => "incomplete-stale-revision",
                "blank-hidden" => "incomplete-blank-hidden",
                "unknown-no-frame" => "incomplete-unknown-no-frame",
                "unknown-output-ungrounded" => "incomplete-unknown-output-ungrounded",
                "classifier-omitted-minimized" => "incomplete-minimized",
                "classifier-omitted-hidden-app" => "incomplete-hidden-app",
                _ => "incomplete-unknown",
            };
            emit_diag(
                shared,
                *app_pid,
                0,
                obs_seq,
                kind::REPORT_ROSTER,
                outcome,
                *roster_len,
            );
        }
    }
    if !any_incomplete {
        emit_diag(
            shared,
            pid,
            generation,
            obs_seq,
            kind::REPORT_ROSTER,
            "complete",
            apps.len() as u64,
        );
    }
}

/// One per-scope applied-reply authority line: held AND clear are both
/// explicit. `authority` is a static token (`controlled|untouched|held-*`);
/// `index` is the scope's position in the applied reply's `scopes` order
/// (joined via `ref`). Disabled: one flag load.
pub(crate) fn emit_reply_scope(
    shared: &Arc<Mutex<crate::runtime_shared::Shared>>,
    pid: u32,
    generation: u64,
    seq: u64,
    authority: &'static str,
    index: u64,
) {
    if !enabled() {
        return;
    }
    emit_diag(shared, pid, generation, seq, kind::REPLY, authority, index);
}

/// One close-pipeline stage record. `count` is `now_us` on the common
/// monotonic clock so trigger/snapshot/report/reply/frame/focus stamps
/// subtract. Disabled: one flag load plus (when enabled) one clock read.
pub(crate) fn emit_close(
    shared: &Arc<Mutex<crate::runtime_shared::Shared>>,
    pid: u32,
    generation: u64,
    ref_id: u64,
    stage: &'static str,
) {
    if !enabled() {
        return;
    }
    emit_diag(
        shared,
        pid,
        generation,
        ref_id,
        kind::CLOSE,
        stage,
        crate::stage_timing::now_us(),
    );
}

/// Pre-transaction entry class for a frame record: `none` (legs ran),
/// `degenerate` (invalid frame), or `entry-fence` (whole-transaction
/// refusal before any leg).
const ENTRY_NONE: &str = "none";
/// Invalid frame refused before any leg.
pub const ENTRY_DEGENERATE: &str = "degenerate";
/// Whole-transaction fence refusal before any leg (no write, no readback).
pub const ENTRY_FENCE: &str = "entry-fence";

/// Format one owner-thread frame diagnostic line (pure; the caller gates on
/// [`enabled`] and prints to stderr). Attempted setters are legs that
/// issued an AX write; entry exits attempt none. Readbacks are the bounded
/// per-leg re-reads that actually ran.
#[must_use]
pub fn format_frame_diag(
    pid: u32,
    wid: u32,
    revision: u64,
    entry: &str,
    completion: &str,
    report: &crate::native_tile_backend::PartialWriteReport,
) -> String {
    use crate::native_tile_backend::LegOutcome;
    let class_of = |leg: &LegOutcome| match leg {
        LegOutcome::Applied => "applied",
        LegOutcome::Refused(_) => "refused",
        LegOutcome::SkippedSafety(_) => "safety",
        LegOutcome::SkippedRedundant => "redundant",
    };
    let cause_of = |leg: &LegOutcome| match leg {
        LegOutcome::Refused(reason) | LegOutcome::SkippedSafety(reason) => diag_cause(reason),
        LegOutcome::Applied | LegOutcome::SkippedRedundant => "-",
    };
    let early = entry != ENTRY_NONE;
    let attempted_leg =
        |leg: &LegOutcome| matches!(leg, LegOutcome::Applied | LegOutcome::Refused(_));
    let setters_attempted = if early {
        0
    } else {
        [&report.size_first, &report.position, &report.size]
            .iter()
            .filter(|leg| attempted_leg(leg))
            .count()
    };
    let setters_applied = if early {
        0
    } else {
        [&report.size_first, &report.position, &report.size]
            .iter()
            .filter(|leg| matches!(leg, LegOutcome::Applied))
            .count()
    };
    let safety_at = |leg: &LegOutcome| matches!(leg, LegOutcome::SkippedSafety(_));
    let readbacks_attempted = if early {
        0
    } else {
        1 + u64::from(!safety_at(&report.position)) + u64::from(!safety_at(&report.size))
    };
    let readbacks_unreadable = if early {
        0
    } else {
        let mut unreadable = 0u64;
        if report.leg_readbacks[0].is_none() {
            unreadable += 1;
        }
        if !safety_at(&report.position) && report.leg_readbacks[1].is_none() {
            unreadable += 1;
        }
        if !safety_at(&report.size) && report.leg_readbacks[2].is_none() {
            unreadable += 1;
        }
        unreadable
    };
    format!(
        "tiler-diag stage=frame stamp_us={} pid={pid} wid={wid} rev={revision} entry={entry} completion={completion} attempted={setters_attempted} applied={setters_applied} readbacks={readbacks_attempted} unreadable={readbacks_unreadable} leg0={}:{} leg1={}:{} leg2={}:{}",
        crate::stage_timing::now_us(),
        class_of(&report.size_first),
        cause_of(&report.size_first),
        class_of(&report.position),
        cause_of(&report.position),
        class_of(&report.size),
        cause_of(&report.size),
    )
}

/// Emit one owner-thread frame diagnostic line. Disabled: one flag load, no
/// formatting, no output. Call at every `apply_scoped_frame` return where
/// the report plus completion is in scope (both pre-transaction early
/// returns and the normal completion).
pub fn emit_frame_diag(
    pid: u32,
    wid: u32,
    revision: u64,
    entry: &'static str,
    completion: &'static str,
    report: &crate::native_tile_backend::PartialWriteReport,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "{}",
        format_frame_diag(pid, wid, revision, entry, completion, report)
    );
}

// ---------------------------------------------------------------------------
// Explicit `tiler-diag` stderr lines (same `key=value` shape as the
// `frame-set` line): every line carries the common-clock `stamp_us` so
// trigger->snapshot->absence->report->reply->frame/focus stamps subtract.
// All gated on [`enabled`]; disabled costs one flag load. Identifiers are
// the existing opaque strings (request ids, `pid_wid` window ids,
// output/workspace ids, static tokens); empty or whitespace-bearing
// values render as `unavailable`, never Debug dumps. No new native
// queries: every value rides facts already in scope at the call site.
// ---------------------------------------------------------------------------

/// Opaque-id sanitizer for `tiler-diag` lines: the existing opaque strings
/// pass through; anything empty or whitespace-bearing (never in practice)
/// renders as `unavailable` so the `key=value` shape stays greppable.
#[must_use]
pub fn diag_id(s: &str) -> &str {
    if s.is_empty() || s.bytes().any(|b| b.is_ascii_whitespace()) {
        "unavailable"
    } else {
        s
    }
}

/// One focus-route line per exit: the exact request id, cause, and target
/// plus the retained observed-focus identity that decided it. `seq` is the
/// dispatch seq (the report join); `outcome` is the exact route predicate
/// token. Disabled: one flag load plus one clock read.
pub(crate) fn emit_focus_line(
    id: &str,
    cause: tiler_core::world_report::FocusRequestCause,
    target: &str,
    observed: Option<&str>,
    known: bool,
    seq: u64,
    outcome: &'static str,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=focus stamp_us={} id={} cause={} target={} observed={} known={} report={} outcome={} provenance=retained",
        crate::stage_timing::now_us(),
        diag_id(id),
        cause.as_str(),
        diag_id(target),
        observed.map(diag_id).unwrap_or("-"),
        u64::from(known),
        seq,
        outcome,
    );
}

/// One held-scope line: the ring scope index alone cannot identify the
/// domain, so the scope's explicit output/workspace rides here (native
/// binding as bound, logical id as planned, `unavailable` when unmapped).
/// `obs` is the observation seq, `reply` the dispatch seq. Only held
/// scopes log; the per-reply cover line below keeps the clear side
/// explicit. Disabled: one flag load plus one clock read.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_hold_scope(
    index: u64,
    out: &str,
    ws: &str,
    native: &str,
    logical: &str,
    authority: &'static str,
    obs_seq: u64,
    dispatch_seq: u64,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=hold stamp_us={} index={} out={} ws={} native={} logical={} authority={} obs={} reply={}",
        crate::stage_timing::now_us(),
        index,
        diag_id(out),
        diag_id(ws),
        diag_id(native),
        diag_id(logical),
        authority,
        obs_seq,
        dispatch_seq,
    );
}

/// One per-reply hold-cover line: the coverage that held the scopes above
/// (or held nothing). `unreadable` lists unreadable native domains as
/// `out:ws` (`none` when all readable, `unavailable` with no context);
/// `skipped` lists skipped displays as `cg_id:cause-token`; `roster` lists
/// incomplete apps as `pid:gen:cause-token` plus unreadable member rows as
/// `pid_wid:reason gen=<app-gen> home=out:ws` (`complete` when nothing is
/// held). `req` is the request seq joining the close-report line, `gap` the
/// session coverage gap at dispatch, `fg` the opaque retained foreground
/// binding (`unavailable` when unknown), `members_readable` /
/// `members_unreadable` the roster member counts by observed presence. All
/// from existing retained/native facts. Disabled: one flag load plus one
/// clock read.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_hold_cover(
    unreadable: &str,
    skipped: &str,
    roster: &str,
    obs_seq: u64,
    dispatch_seq: u64,
    req_seq: u64,
    coverage_gap: bool,
    foreground: &str,
    members_readable: u64,
    members_unreadable: u64,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=hold-cover stamp_us={} obs={} reply={} req={} gap={} fg={} members_readable={} members_unreadable={} unreadable={} skipped={} roster={}",
        crate::stage_timing::now_us(),
        obs_seq,
        dispatch_seq,
        req_seq,
        u64::from(coverage_gap),
        diag_id(foreground),
        members_readable,
        members_unreadable,
        unreadable,
        skipped,
        roster,
    );
}

/// One trigger-receipt line on the owner pump: the reread reason that
/// fired (`initial|notification|topology|arrival|backstop`; the
/// notification itself carries no window data, so a notified wake logs
/// `notification-coalesced`). Disabled: one flag load plus one clock read.
pub(crate) fn emit_close_trigger(pid: u32, trigger: &str, rev: u64, notified: bool) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=close-trigger stamp_us={} pid={} trigger={} rev={} notif={}",
        crate::stage_timing::now_us(),
        pid,
        trigger,
        rev,
        if notified {
            "notification-coalesced"
        } else {
            "none"
        },
    );
}

/// Unique per-app snapshot sequence joining one snapshot start/end pair.
static SNAP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Mint the next snapshot join id. Lock-free monotonic; wraps only after
/// u64::MAX snapshots (never in practice).
#[must_use]
pub(crate) fn next_snap() -> u64 {
    SNAP_SEQ
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .wrapping_add(1)
}

/// One snapshot-start line: the reread reason plus the join id the end
/// line echoes. Disabled: one flag load plus one clock read.
pub(crate) fn emit_snap_start(snap: u64, pid: u32, rev: u64, trigger: &str) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=snapshot-start stamp_us={} snap={} pid={} rev={} trigger={}",
        crate::stage_timing::now_us(),
        snap,
        pid,
        rev,
        trigger,
    );
}

/// One snapshot-end line joining [`emit_snap_start`]: `ok|err` only, never
/// window detail. Disabled: one flag load plus one clock read.
pub(crate) fn emit_snap_end(snap: u64, pid: u32, outcome: &'static str, cause: &'static str) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=snapshot-end stamp_us={} snap={} pid={} outcome={} cause={}",
        crate::stage_timing::now_us(),
        snap,
        pid,
        outcome,
        cause,
    );
}

/// First-observed absence line at the non-covering prune: the `pruned`
/// count that proves loss for exactly this lifetime BEFORE the covering
/// close confirms it downstream — the stamp delta to the later
/// report/reply/frame lines measures the close delay. `obs` is the
/// observation seq. Disabled: one flag load plus one clock read.
pub(crate) fn emit_close_absence(pid: u32, generation: u64, seq: u64, pruned: u64, covering: bool) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=close-absence stamp_us={} pid={} gen={} obs={} pruned={} covering={}",
        crate::stage_timing::now_us(),
        pid,
        generation,
        seq,
        pruned,
        u64::from(covering),
    );
}

/// Report join line: the ring `report-sent` stage keys on the observation
/// seq while `reply-applied` keys on the dispatch seq, so the dispatch,
/// observation, and request seqs ride explicitly here. Disabled: one flag
/// load plus one clock read.
pub(crate) fn emit_close_report(
    dispatch_seq: u64,
    obs_seq: u64,
    req_seq: u64,
    trace: u64,
    pid: u32,
    generation: u64,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=close-report stamp_us={} dispatch={} obs={} req={} trace={} pid={} gen={}",
        crate::stage_timing::now_us(),
        dispatch_seq,
        obs_seq,
        req_seq,
        trace,
        pid,
        generation,
    );
}

/// Reply join line: echoes the dispatch seq alongside the observation seq
/// the reply was based on, joining [`emit_close_report`]. Disabled: one
/// flag load plus one clock read.
pub(crate) fn emit_close_reply(dispatch_seq: u64, obs_seq: u64, req_seq: u64) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=close-reply stamp_us={} dispatch={} obs={} req={}",
        crate::stage_timing::now_us(),
        dispatch_seq,
        obs_seq,
        req_seq,
    );
}

/// One frame-batch line where the vectors are queued but no native write
/// has run yet (entry counts = queued counts; the genuine completion line
/// below records the worker-side terminal accounting). Disabled: one flag
/// load plus one clock read.
pub(crate) fn emit_batch_queued(frames: u64, ops: u64, overlay_skipped: u64, seq: u64) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=batch-queued stamp_us={} dispatch={} frames={} ops={} overlay_skipped={}",
        crate::stage_timing::now_us(),
        seq,
        frames,
        ops,
        overlay_skipped,
    );
}

/// One genuine frame-batch completion line on the worker after the frame
/// loop plus the post-batch snapshot: `first` is the batch's first command
/// id (0 when empty), `cmds` the terminally accounted command count,
/// `readback` the post-batch snapshot outcome (`snapshot|failed`), and
/// `traces` the comma-joined parent trace ids (empty when none). Joins the
/// report's trace, not the worker-local observation seq. Disabled: one flag load
/// plus one clock read.
pub(crate) fn emit_batch_done(
    pid: u32,
    seq: u64,
    first_cmd: u64,
    cmds: u64,
    readback: &'static str,
    traces: &str,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=batch-done stamp_us={} pid={} worker_obs={} first={} cmds={} readback={} traces={}",
        crate::stage_timing::now_us(),
        pid,
        seq,
        first_cmd,
        cmds,
        readback,
        if traces.is_empty() { "none" } else { traces },
    );
}

/// One per-window first-AX-absence line at the scoped observation: a
/// retained roster window missing from BOTH the snapshot windows and the
/// classifier exclusions (a missing candidate for exactly this observation,
/// before any roster application). `cg` is the CG-oracle evidence for the
/// wid (`owned|absent|unknown`), `home` the retained domain (`out:ws`,
/// `unavailable` when unhomed), `fate` the read-only mirror verdict
/// (`retained-cg-alive|retained-unknown-oracle|pruned|preserved-hidden`),
/// `covering` whether the snapshot claims whole-scope truth. Every missing
/// candidate logs (analysis selects the first); nothing is destroyed here.
/// Disabled: one gate plus one clock read.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_close_absence_id(
    pid: u32,
    generation: u64,
    obs: u64,
    wid: u32,
    cg: &str,
    home: &str,
    fate: &'static str,
    covering: bool,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=close-absence stamp_us={} pid={} gen={} obs={} wid={} cg={} home={} fate={} covering={}",
        crate::stage_timing::now_us(),
        pid,
        generation,
        obs,
        wid,
        diag_id(cg),
        diag_id(home),
        fate,
        u64::from(covering),
    );
}

/// One per-window prune line when a window leaves the roster: the
/// individual id behind a prune count. `covering` names the branch that
/// pruned it (true = covering snapshot, false = non-covering current-homed
/// oracle prune). Disabled: one flag load plus one clock read.
pub(crate) fn emit_close_pruned(pid: u32, generation: u64, obs: u64, id: &str, covering: bool) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=close-pruned stamp_us={} pid={} gen={} obs={} id={} covering={}",
        crate::stage_timing::now_us(),
        pid,
        generation,
        obs,
        diag_id(id),
        u64::from(covering),
    );
}

/// One observation-rejection line naming the live and snapshot revisions
/// that disagreed (`None` renders `unavailable`: no context has no live
/// revision). Disabled: one flag load plus one clock read.
pub(crate) fn emit_observe_reject(
    pid: u32,
    generation: u64,
    obs: u64,
    snap_rev: Option<u64>,
    live_rev: Option<u64>,
    reason: &'static str,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=observe-reject stamp_us={} pid={} gen={} obs={} snap={} live={} reason={}",
        crate::stage_timing::now_us(),
        pid,
        generation,
        obs,
        snap_rev
            .map(|r| r.to_string())
            .unwrap_or_else(|| "unavailable".to_owned()),
        live_rev
            .map(|r| r.to_string())
            .unwrap_or_else(|| "unavailable".to_owned()),
        reason,
    );
}

/// One owner focus-result line on the worker result path: the mechanical
/// outcome of the actual AX attempt plus its exact cause token (never the
/// request alone). `cmd` is the focus request id. No new reads: the
/// completion already ran. Disabled: one flag load plus one clock read.
pub(crate) fn emit_focus_done(pid: u32, cmd: u64, completion: &crate::runtime_types::AxCompletion) {
    if !enabled() {
        return;
    }
    let (outcome, cause) = match completion {
        crate::runtime_types::AxCompletion::Applied { .. } => ("applied", "-"),
        crate::runtime_types::AxCompletion::PartialApplied { .. } => ("partial-applied", "-"),
        crate::runtime_types::AxCompletion::Timeout { .. } => ("timeout", "messaging-timeout"),
        crate::runtime_types::AxCompletion::Failed { reason } => ("failed", diag_cause(reason)),
    };
    eprintln!(
        "tiler-diag stage=focus-done stamp_us={} pid={} cmd={} outcome={} cause={} provenance=owner",
        crate::stage_timing::now_us(),
        pid,
        cmd,
        outcome,
        cause,
    );
}

/// One owner focus-feedback line per no-call terminal: the typed setter +
/// reason that withheld the AX attempt plus the pre/post identities already
/// in scope at the build site. Covers every no-call exit in one place (the
/// `dispatched` path reports via `focus-done` instead). `pre`/`post` are
/// opaque window ids (`None` = empty there). No new reads: every value
/// rides the build closure's existing facts. Disabled: one flag load plus
/// one clock read.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_focus_feedback(
    pid: u32,
    cmd: u64,
    close_id: &str,
    target: &str,
    setter: tiler_core::world_report::VisibilitySetter,
    reason: tiler_core::world_report::VisibilityReason,
    pre: Option<&str>,
    post: Option<&str>,
    proven: bool,
    attempt_ms: u64,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "tiler-diag stage=focus-feedback stamp_us={} pid={} cmd={} close={} target={} setter={} reason={} pre={} post={} proven={} attempt_ms={}",
        crate::stage_timing::now_us(),
        pid,
        cmd,
        diag_id(close_id),
        diag_id(target),
        setter.as_str(),
        reason_token(reason),
        pre.map(diag_id).unwrap_or("unavailable"),
        post.map(diag_id).unwrap_or("unavailable"),
        u64::from(proven),
        attempt_ms,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::world_report::{FocusRequestCause, VisibilityReason};

    #[test]
    fn schema_kinds_share_diag_prefix() {
        for k in [
            kind::FOCUS_EXPLICIT,
            kind::FOCUS_ADMISSION,
            kind::FOCUS_FOLLOWS,
            kind::FOCUS_CLOSE,
            kind::REPORT,
            kind::REPORT_VIEW,
            kind::REPORT_ROSTER,
            kind::REPLY,
            kind::CLOSE,
        ] {
            assert!(k.starts_with("diag-"), "{k:?}");
        }
    }

    #[test]
    fn cause_kinds_cover_every_request_cause() {
        assert_eq!(
            focus_cause_kind(FocusRequestCause::ExplicitCommand),
            kind::FOCUS_EXPLICIT
        );
        assert_eq!(
            focus_cause_kind(FocusRequestCause::AutomaticAdmission),
            kind::FOCUS_ADMISSION
        );
        assert_eq!(
            focus_cause_kind(FocusRequestCause::FocusFollowsCursor),
            kind::FOCUS_FOLLOWS
        );
        assert_eq!(
            focus_cause_kind(FocusRequestCause::PositiveClose),
            kind::FOCUS_CLOSE
        );
    }

    #[test]
    fn diag_ids_sanitize_without_leaking() {
        assert_eq!(diag_id("4242_7"), "4242_7");
        assert_eq!(diag_id("op-9"), "op-9");
        assert_eq!(diag_id(""), "unavailable");
        assert_eq!(diag_id("a b"), "unavailable");
        assert_eq!(diag_id("a\tb"), "unavailable");
    }

    #[test]
    fn focus_causes_have_stable_wire_tokens() {
        assert_eq!(
            FocusRequestCause::ExplicitCommand.as_str(),
            "explicit-command"
        );
        assert_eq!(
            FocusRequestCause::AutomaticAdmission.as_str(),
            "automatic-admission"
        );
        assert_eq!(
            FocusRequestCause::FocusFollowsCursor.as_str(),
            "focus-follows-cursor"
        );
        assert_eq!(FocusRequestCause::PositiveClose.as_str(), "positive-close");
    }

    #[test]
    fn snap_ids_mint_unique_joins() {
        assert!(next_snap() < next_snap());
    }

    #[test]
    fn reason_tokens_cover_every_visibility_reason() {
        let tokens = [
            (VisibilityReason::Empty, "empty"),
            (VisibilityReason::IdentityStale, "identity-stale"),
            (VisibilityReason::ScopeHeld, "scope-held"),
            (VisibilityReason::Unreadable, "unreadable"),
            (VisibilityReason::Unsupported, "unsupported"),
            (
                VisibilityReason::PreimageUnavailable,
                "preimage-unavailable",
            ),
            (VisibilityReason::ForegroundChanged, "foreground-changed"),
            (VisibilityReason::Fullscreen, "fullscreen"),
            (VisibilityReason::Game, "game"),
            (VisibilityReason::ShowBlocked, "show-blocked"),
            (VisibilityReason::FocusUnproven, "focus-unproven"),
            (VisibilityReason::Superseded, "superseded"),
            (VisibilityReason::NativeRefused, "native-refused"),
        ];
        for (reason, token) in tokens {
            assert_eq!(reason_token(reason), token);
            assert_eq!(
                deferred_refusal_outcome(reason).strip_prefix("deferred-refused-"),
                Some(token)
            );
        }
    }

    #[test]
    fn roster_causes_map_exactly_or_unknown() {
        for known in [
            "cg-alive",
            "cg-unreadable",
            "no-context",
            "stale-revision",
            "blank-hidden",
        ] {
            assert_eq!(roster_cause_token(known), known);
        }
        assert_eq!(roster_cause_token(""), "complete");
        assert_eq!(roster_cause_token("CG-ALIVE"), "unknown");
        assert_eq!(roster_cause_token("future-cause"), "unknown");
        assert_eq!(roster_cause_token("pid 1 title leak"), "unknown");
    }

    #[test]
    fn classifier_preserves_every_safety_predicate() {
        // Each fence refusal keeps an exact token instead of collapsing
        // into the sanitizer's failed-set/failed-snapshot/failed-backend
        // buckets (spot-checked against the bucket each would fall in).
        let cases = [
            ("Secure Input active; refusing", "secure-input-active"),
            (
                "locked session (loginwindow frontmost); refusing",
                "session-loginwindow-locked",
            ),
            (
                "session unknown; refusing without a readable unlocked desktop",
                "session-unknown",
            ),
            (
                "locked session (window server); refusing",
                "session-server-locked",
            ),
            ("refusing window id 0", "wid-zero"),
            ("worker stopped mid-plan; withholding", "worker-stopped"),
            (
                "superseded by a newer desired set mid-plan; withholding",
                "frame-superseded",
            ),
            ("stale command pid 9 != bound pid 7; refusing", "stale-pid"),
            ("pid mismatch (stale command)", "stale-pid"),
            ("refusing Ghostty at the effect boundary", "scope-ghostty"),
            (
                "refusing own bundle at the effect boundary",
                "scope-own-bundle",
            ),
            (
                "refusing own helper bundle at the effect boundary",
                "scope-helper-bundle",
            ),
            ("out of scope: refusing write", "scope-out-of-scope"),
            ("unknown bundle: refusing write", "scope-unknown-bundle"),
            ("refusing own-process identity", "identity-own-process"),
            ("refusing Ghostty identity", "identity-ghostty"),
            ("refusing own-bundle identity", "identity-own-bundle"),
            (
                "refusing own helper-bundle identity (unscoped product run; scoped fixtures list it explicitly)",
                "identity-helper-bundle",
            ),
            ("out of scope: refusing owner", "identity-out-of-scope"),
            ("unknown bundle: refusing owner", "identity-unknown-bundle"),
            (
                "identity launch date unreadable; refusing",
                "identity-no-launch-date",
            ),
            (
                "refusing identity without an executable path",
                "identity-no-exe",
            ),
            (
                "refusing identity without a launch date (no PID-reuse protection)",
                "identity-no-launch-date",
            ),
            (
                "identity drift (possible PID reuse); refusing",
                "identity-drift",
            ),
            ("refusing out-of-range pid 0", "pid-out-of-range"),
            ("fence target mismatch; refusing", "fence-target-mismatch"),
            ("CG window list unreadable; refusing", "cg-unreadable"),
            ("wid absent from CG list; refusing", "cg-absent"),
            ("wid owned by another pid; refusing", "cg-owner-mismatch"),
            (
                "Space membership unreadable; refusing",
                "membership-unreadable",
            ),
            (
                "window not exactly on one Space; refusing",
                "membership-multi-space",
            ),
            (
                "window not exactly on the current user Space; refusing",
                "membership-off-current-space",
            ),
            (
                "current Space homed by != 1 domain; refusing",
                "space-home-ambiguous",
            ),
            ("planned domain drifted mid-plan; refusing", "domain-drift"),
            (
                "display rows unreadable; refusing",
                "display-rows-unreadable",
            ),
            (
                "display UUID unknown; refusing positional home",
                "display-uuid-unknown",
            ),
            ("current Space drifted mid-plan; refusing", "space-drift"),
            (
                "current Space no longer user-typed; refusing",
                "space-non-user",
            ),
            (
                "pid 9: no effect hook bound; refusing without a live revision source",
                "no-hook",
            ),
            ("native context cleared; refusing", "ctx-cleared"),
            ("no native context published; refusing", "ctx-cleared"),
            (
                "topology revision drift (want 7, live 9); superseded",
                "revision-drift",
            ),
            ("topology revision drift; superseded", "revision-drift"),
            (
                "minimized: withholding geometry write",
                "minimized-withheld",
            ),
            (
                "hidden app: withholding geometry write",
                "hidden-app-withheld",
            ),
            (
                "fullscreen overlay: withholding geometry write",
                "fullscreen-withheld",
            ),
            ("fullscreen unreadable; refusing", "fullscreen-unreadable"),
            ("minimized unreadable; refusing", "minimized-unreadable"),
            (
                "pid 9: refusing whole snapshot (wid 7: membership unreadable)",
                "snapshot-incomplete",
            ),
            (
                "subscription sync pid 9: kAXErrorFailure",
                "observer-sync-failed",
            ),
            ("window identity join failed", "identity-join-failed"),
            ("no AX element for pid", "no-ax-element"),
            (
                "snapshot pid 9: messaging timeout unset (2000ms)",
                "messaging-timeout-unset",
            ),
            ("snapshot pid 9: messaging timeout", "messaging-timeout"),
            ("wid not joined to owned pid", "wid-not-joined"),
            ("degenerate frame", "degenerate-frame"),
            ("frame actuation is macOS-only", "off-macos-frame"),
            (
                "focus dispatch unsupported by backend",
                "focus-unsupported-backend",
            ),
        ];
        for (reason, token) in cases {
            assert_eq!(diag_cause(reason), token, "{reason:?}");
        }
    }

    #[test]
    fn classifier_falls_back_to_sanitizer_for_ax_and_generic() {
        // The kAXError allowlist survives (specific OS cause preserved),
        // and generic backend detail still buckets without leaking.
        assert_eq!(diag_cause("set: kAXErrorFailure"), "failed-ax-failure");
        assert_eq!(diag_cause("set: AXError 99999"), "failed-set");
        assert_eq!(
            diag_cause("pid 1234 / titled window leaked"),
            "failed-backend"
        );
        assert_eq!(
            diag_cause("messaging timeout unset (2000ms)"),
            "messaging-timeout-unset"
        );
    }

    fn refused(reason: &str) -> crate::native_tile_backend::LegOutcome {
        crate::native_tile_backend::LegOutcome::Refused(reason.to_owned())
    }

    fn report_of(
        size_first: crate::native_tile_backend::LegOutcome,
        position: crate::native_tile_backend::LegOutcome,
        size: crate::native_tile_backend::LegOutcome,
        readbacks: [Option<crate::ffi::AxFrame>; 3],
    ) -> crate::native_tile_backend::PartialWriteReport {
        crate::native_tile_backend::PartialWriteReport {
            pid: 4242,
            wid: 7,
            size_first,
            position,
            size,
            leg_readbacks: readbacks,
            readback: None,
            readback_unreadable: true,
        }
    }

    fn frame() -> crate::ffi::AxFrame {
        crate::ffi::AxFrame {
            x: 8.0,
            y: 38.0,
            w: 1108.0,
            h: 1123.0,
        }
    }

    #[test]
    fn frame_line_counts_normal_path_with_unreadable_readbacks() {
        use crate::native_tile_backend::LegOutcome;
        use crate::runtime_types::AxCompletion;
        let report = report_of(
            LegOutcome::Applied,
            LegOutcome::Applied,
            LegOutcome::Applied,
            [None, None, None],
        );
        let line = format_frame_diag(4242, 7, 9, ENTRY_NONE, "applied", &report);
        assert!(
            line.starts_with("tiler-diag stage=frame stamp_us="),
            "{line:?}"
        );
        for token in [
            "pid=4242",
            "wid=7",
            "rev=9",
            "entry=none",
            "completion=applied",
            "attempted=3",
            "applied=3",
            "readbacks=3",
            "unreadable=3",
            "leg0=applied:-",
            "leg1=applied:-",
            "leg2=applied:-",
        ] {
            assert!(line.contains(token), "{token:?} in {line:?}");
        }
        let _ = AxCompletion::Applied { elapsed_ms: 1 };
    }

    #[test]
    fn frame_line_counts_safety_skip_without_its_readback() {
        use crate::native_tile_backend::LegOutcome;
        let f = frame();
        let report = report_of(
            LegOutcome::Applied,
            LegOutcome::SkippedSafety("hidden app: withholding geometry write".to_owned()),
            LegOutcome::Applied,
            [Some(f), None, Some(f)],
        );
        let line = format_frame_diag(4242, 7, 9, ENTRY_NONE, "failed", &report);
        for token in [
            "attempted=2",
            "applied=2",
            "readbacks=2",
            "unreadable=0",
            "leg1=safety:hidden-app-withheld",
        ] {
            assert!(line.contains(token), "{token:?} in {line:?}");
        }
    }

    #[test]
    fn frame_line_counts_entry_exits_as_zero_attempt() {
        let report = report_of(
            refused("window not exactly on the current user Space; refusing"),
            refused("window not exactly on the current user Space; refusing"),
            refused("window not exactly on the current user Space; refusing"),
            [None, None, None],
        );
        let line = format_frame_diag(4242, 7, 9, ENTRY_FENCE, "failed", &report);
        for token in [
            "entry=entry-fence",
            "attempted=0",
            "applied=0",
            "readbacks=0",
            "unreadable=0",
            "leg0=refused:membership-off-current-space",
        ] {
            assert!(line.contains(token), "{token:?} in {line:?}");
        }
        let degenerate = report_of(
            refused("degenerate frame"),
            refused("degenerate frame"),
            refused("degenerate frame"),
            [None, None, None],
        );
        let dline = format_frame_diag(4242, 7, 9, ENTRY_DEGENERATE, "failed", &degenerate);
        assert!(dline.contains("entry=degenerate"), "{dline:?}");
        assert!(dline.contains("leg0=refused:degenerate-frame"), "{dline:?}");
    }
}
