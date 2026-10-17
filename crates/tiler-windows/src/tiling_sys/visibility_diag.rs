#![forbid(unsafe_code)]
use super::*;

use crate::product_hide::ProductTeardown;

/// Bounded per-select reveal diagnostics cap: one `workspace-reveal` line per
/// entering member up to this cap; the remainder report through one
/// `workspace-reveal-summary` line with an honest omitted count.
/// Reason: bounds normal-log volume for large workspaces (mirrors the hide cap).
pub(super) const REVEAL_DIAG_PER_SELECT: usize = 64;
/// Reason: caps entries per batched visibility event to bound normal-log volume.
pub(super) const VIS_DIAG_MAX_ENTRIES: usize = 16;
/// Reason: spaces visibility batches to bound normal-log volume.
pub(super) const VIS_DIAG_INTERVAL: Duration = Duration::from_secs(1);
/// Reason: caps entries per batched exception event to bound normal-log volume.
pub(super) const EXC_DIAG_MAX_ENTRIES: usize = 16;
/// Reason: spaces exception batches to bound normal-log volume.
pub(super) const EXC_DIAG_INTERVAL: Duration = Duration::from_secs(1);
/// Reason: spaces rehide-sweep summaries to bound normal-log volume: the
/// quiet-tick sweep evaluates every hidden claim, so a per-pass cap alone
/// would still emit every tick on persistent drift.
pub(super) const REHIDE_DIAG_INTERVAL: Duration = Duration::from_secs(1);
/// Reason: caps windows listed per rehide summary; bucket counts stay honest.
pub(super) const REHIDE_DIAG_MAX_ENTRIES: usize = 16;
/// Reason: caps opaque windows remembered per outcome bucket between emits.
const REHIDE_DIAG_MAX_TOKENS_PER_BUCKET: usize = 4;

const LANE_FLOAT: u8 = 1;
const LANE_STICKY: u8 = 2;
const LANE_BORN: u8 = 4;
const LANE_AUTO_FIXED: u8 = 8;
const LANE_ENGINE: u8 = 16;

/// Cohesive normal-level visibility/exception diagnostics state: latest
/// samples, emitted baselines, throttle clocks, fair emission order,
/// throttled-round tokens, and lost-intermediate flags. Pruned by live
/// membership alongside the hint caches.
#[derive(Default)]
pub(super) struct VisibilityDiagnostics {
    pub(super) vis_pending: HashMap<String, VisibilitySample>,
    pub(super) vis_emitted: HashMap<String, VisibilitySample>,
    pub(super) vis_last_emit: Option<Instant>,
    pub(super) vis_emit_seq: HashMap<String, u64>,
    pub(super) vis_emit_clock: u64,
    pub(super) vis_coalesced: HashSet<String>,
    pub(super) vis_lost: HashSet<String>,
    pub(super) exc_masks: HashMap<String, u8>,
    pub(super) exc_pending: HashMap<String, ExceptionSample>,
    pub(super) exc_emitted: HashMap<String, u8>,
    pub(super) exc_last_emit: Option<Instant>,
    pub(super) exc_emit_seq: HashMap<String, u64>,
    pub(super) exc_emit_clock: u64,
    pub(super) exc_coalesced: HashSet<String>,
    pub(super) exc_lost: HashSet<String>,
    /// Per-second rehide-sweep aggregation: pending (outcome, detail) buckets
    /// with honest counts plus capped opaque sample windows, and the last
    /// summary emission clock. Never pruned by membership: buckets drain on
    /// the next emission and carry no per-window state.
    pub(super) rehide_pending:
        std::collections::BTreeMap<(&'static str, &'static str), RehideBucket>,
    pub(super) rehide_last_emit: Option<Instant>,
}

/// One rehide-sweep bucket: honest instance count plus capped opaque sample
/// windows (unknown-token instances count with no sample listed).
#[derive(Default)]
pub(super) struct RehideBucket {
    pub(super) count: usize,
    pub(super) tokens: Vec<String>,
}

/// One managed visibility sample: the coarse lane state plus independent
/// native evidence (`visible`, `cloaked`), the product-hide claim flag, and
/// overlay facts. `tick` and `observed_ms` are provenance only and never
/// participate in change dedupe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VisibilitySample {
    pub(super) state: &'static str,
    pub(super) reason: &'static str,
    pub(super) visible: Option<bool>,
    pub(super) cloaked: Option<bool>,
    pub(super) hide_claim: bool,
    pub(super) overlay: &'static str,
    pub(super) tick: u64,
    pub(super) observed_ms: u64,
}

impl VisibilitySample {
    fn stable_eq(&self, other: &Self) -> bool {
        self.state == other.state
            && self.reason == other.reason
            && self.visible == other.visible
            && self.cloaked == other.cloaked
            && self.hide_claim == other.hide_claim
            && self.overlay == other.overlay
    }
}

/// One exception-lane sample: the lane bitmask plus provenance. Provenance
/// never participates in change dedupe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExceptionSample {
    pub(super) mask: u8,
    pub(super) tick: u64,
    pub(super) observed_ms: u64,
}

fn lane_names(mask: u8) -> Vec<&'static str> {
    let mut lanes = Vec::new();
    if mask & LANE_FLOAT != 0 {
        lanes.push("intentional-float");
    }
    if mask & LANE_STICKY != 0 {
        lanes.push("sticky");
    }
    if mask & LANE_BORN != 0 {
        lanes.push("born-fullscreen");
    }
    if mask & LANE_AUTO_FIXED != 0 {
        lanes.push("automatic-fixed");
    }
    if mask & LANE_ENGINE != 0 {
        lanes.push("engine-exception");
    }
    lanes
}

fn exc_reason(mask: u8) -> &'static str {
    if mask & LANE_STICKY != 0 {
        "sticky-lane"
    } else if mask & LANE_FLOAT != 0 {
        "intentional-float-lane"
    } else if mask & LANE_BORN != 0 {
        "born-hold-lane"
    } else if mask & LANE_AUTO_FIXED != 0 {
        "automatic-fixed-lane"
    } else if mask & LANE_ENGINE != 0 {
        "engine-exception-lane"
    } else {
        "tiled"
    }
}

/// Honest per-window reveal outcome from one identity-fenced product reveal.
pub(super) fn reveal_outcome(outcome: &ProductTeardown) -> &'static str {
    match outcome {
        ProductTeardown::Revealed => "revealed",
        ProductTeardown::AlreadyVisible => "already-visible",
        ProductTeardown::Retired => "retired",
        ProductTeardown::Uncertain => "uncertain",
    }
}

/// Categorical (outcome, detail) for one failed reveal `Err`, drawn from the
/// `reveal_product_claim_to` contract vocabulary only: `uncertain:*` keeps
/// the ledger, `refuse:*` is a fence, `absent:*` means verifiably gone, and
/// anything else is a plain failed attempt. Never raw error text.
pub(super) fn reveal_err_outcome(message: &str) -> (&'static str, &'static str) {
    if let Some(rest) = message.strip_prefix("uncertain: ") {
        if rest.starts_with("reveal pre-read") {
            if rest.contains("access-denied") {
                ("uncertain", "reveal-pre-read-access")
            } else if rest.contains("nonce") {
                ("uncertain", "reveal-pre-read-nonce")
            } else {
                ("uncertain", "reveal-pre-read")
            }
        } else if rest.starts_with("reveal held") {
            if rest.contains("absent:") {
                ("uncertain", "reveal-held-absent")
            } else {
                ("uncertain", "reveal-held-error")
            }
        } else if rest.starts_with("reveal post-pid") {
            ("uncertain", "reveal-post-pid")
        } else if rest.starts_with("reveal readback mismatch") {
            ("uncertain", "reveal-readback-mismatch")
        } else if rest.starts_with("reveal readback") {
            if rest.contains("absent:") {
                ("uncertain", "reveal-readback-absent")
            } else if rest.contains("access-denied") {
                ("uncertain", "reveal-readback-access")
            } else {
                ("uncertain", "reveal-readback-error")
            }
        } else if rest.starts_with("visibility not observed") {
            ("uncertain", "reveal-post-unobserved")
        } else {
            ("uncertain", "reveal-uncertain")
        }
    } else if message.starts_with("refuse:") {
        if message.contains("not a product claim") {
            ("refused", "not-product-claim")
        } else {
            ("refused", "reveal-refused")
        }
    } else if message.starts_with("absent:") {
        ("failed", "reveal-absent")
    } else if message.starts_with("error: visibility post failed") {
        ("failed", "reveal-post-failed")
    } else {
        ("failed", "reveal-unknown")
    }
}

/// Categorical (outcome, detail) for one failed `rehide_known_claim` `Err`,
/// drawn from the rehide contract vocabulary only. `rehide-*` details name
/// the failing leg; raw `query_recovery` passthrough errors collapse to the
/// contract category without raw text. Never raw error text.
pub(super) fn rehide_err_outcome(message: &str) -> (&'static str, &'static str) {
    if let Some(rest) = message.strip_prefix("uncertain: rehide pre-read ") {
        let _ = rest;
        ("uncertain", "rehide-pre-read")
    } else if message.starts_with("uncertain: rehide held ") {
        ("uncertain", "rehide-held")
    } else if message.starts_with("uncertain: rehide post-pid") {
        ("uncertain", "rehide-post-pid")
    } else if message.starts_with("uncertain: rehide readback mismatch") {
        ("uncertain", "rehide-readback-mismatch")
    } else if message.starts_with("uncertain: visibility not observed") {
        ("uncertain", "rehide-post-unobserved")
    } else if message.starts_with("uncertain:") {
        ("uncertain", "rehide-uncertain")
    } else if message.starts_with("refuse:") {
        ("refused", "rehide-refused")
    } else if message.starts_with("absent:") {
        ("failed", "rehide-absent")
    } else if message.starts_with("error: visibility post failed") {
        ("failed", "rehide-post-failed")
    } else if message.starts_with("error:") {
        ("failed", "rehide-error")
    } else {
        ("failed", "rehide-unknown")
    }
}

/// Categorical phase detail for a rehide retirement whose identity-safe
/// reveal failed: the actual `reveal_err_outcome` outcome names the result,
/// this names the retirement phase, so a failed retirement can never read as
/// a verified ledger retirement.
pub(super) fn rehide_retired_detail(outcome: &str) -> &'static str {
    match outcome {
        "uncertain" => "retired-reveal-uncertain",
        "refused" => "retired-reveal-refused",
        _ => "retired-reveal-failed",
    }
}

/// Accumulate one rehide-sweep outcome into the per-second buckets: honest
/// instance counts always, opaque sample windows up to the per-bucket cap
/// (same window lists once per bucket). Emits nothing; [`flush_rehide_diag`]
/// emits at most one summary per second.
pub(super) fn note_rehide_diag(
    state: &mut TileLoop,
    token: Option<&str>,
    outcome: &'static str,
    detail: &'static str,
) {
    let bucket = state
        .vis_diag
        .rehide_pending
        .entry((outcome, detail))
        .or_default();
    bucket.count += 1;
    if let Some(token) = token
        && !token.is_empty()
        && bucket.tokens.len() < REHIDE_DIAG_MAX_TOKENS_PER_BUCKET
        && !bucket.tokens.iter().any(|listed| listed == token)
    {
        bucket.tokens.push(token.to_owned());
    }
}

/// Emit one batched `rehide-sweep` summary for outcomes accumulated since the
/// last emission: per-bucket honest counts, up to [`REHIDE_DIAG_MAX_ENTRIES`]
/// opaque sample windows, and the omitted instance remainder. Steady sweeps
/// emit nothing; throttled rounds keep pending for the next emission, so a
/// corrected transient is never silently lost. Deterministic bucket order.
pub(super) fn flush_rehide_diag(state: &mut TileLoop) -> Option<serde_json::Value> {
    let tick = state.tick;
    let diag = &mut state.vis_diag;
    if diag.rehide_pending.is_empty() {
        return None;
    }
    let now = hint_now();
    if diag
        .rehide_last_emit
        .is_some_and(|at| now.saturating_duration_since(at) < REHIDE_DIAG_INTERVAL)
    {
        return None;
    }
    let pending = std::mem::take(&mut diag.rehide_pending);
    diag.rehide_last_emit = Some(now);
    let mut buckets = Vec::with_capacity(pending.len());
    let mut entries = Vec::new();
    let mut total: usize = 0;
    for ((outcome, detail), bucket) in &pending {
        buckets.push(serde_json::json!({
            "outcome": outcome,
            "detail": detail,
            "count": bucket.count,
        }));
        total += bucket.count;
        for token in &bucket.tokens {
            if entries.len() >= REHIDE_DIAG_MAX_ENTRIES {
                break;
            }
            entries.push(serde_json::json!({
                "window": token,
                "outcome": outcome,
                "detail": detail,
            }));
        }
    }
    let omitted = total.saturating_sub(entries.len());
    Some(serde_json::json!({
        "event": "rehide-sweep",
        "tick": tick,
        "buckets": buckets,
        "entries": entries,
        "omitted": omitted,
    }))
}

/// Production `workspace-reveal` payload: opaque token (null when unknown),
/// categorical outcome/detail, tick, action correlation, monotonic elapsed
/// time from the action start, and the operation phase (`None` for the
/// ordinary reveal leg, `Some("rollback")` for narrow-recovery reveals).
pub(super) fn reveal_log_line(
    tick: u64,
    correlation: &str,
    token: Option<&str>,
    outcome: &'static str,
    detail: Option<&'static str>,
    phase: Option<&'static str>,
    elapsed_ms: u64,
) -> serde_json::Value {
    serde_json::json!({
        "event": "workspace-reveal",
        "tick": tick,
        "correlation": correlation,
        "window": token.map_or(serde_json::Value::Null, serde_json::Value::from),
        "outcome": outcome,
        "detail": detail.map_or(serde_json::Value::Null, serde_json::Value::from),
        "phase": phase.map_or(serde_json::Value::Null, serde_json::Value::from),
        "elapsed_ms": elapsed_ms,
    })
}

/// Production `workspace-hide` payload: opaque token (null when unknown),
/// categorical outcome/detail, tick, action correlation, the project-topmost
/// exception flag, and monotonic elapsed time from the action start.
pub(super) fn hide_log_line(
    tick: u64,
    correlation: &str,
    token: Option<&str>,
    outcome: &'static str,
    detail: Option<&'static str>,
    project_topmost: bool,
    elapsed_ms: u64,
) -> serde_json::Value {
    serde_json::json!({
        "event": "workspace-hide",
        "tick": tick,
        "correlation": correlation,
        "window": token.map_or(serde_json::Value::Null, serde_json::Value::from),
        "outcome": outcome,
        "detail": detail.map_or(serde_json::Value::Null, serde_json::Value::from),
        "project_topmost": project_topmost,
        "elapsed_ms": elapsed_ms,
    })
}

/// Bounded per-window hide diagnostic through the shared per-select cap.
/// Silent past the cap; the caller reports the remainder through
/// [`omission_summary`].
pub(super) fn note_hide_diag(
    state: &mut TileLoop,
    ctx: &ActionCtx,
    key: &crate::workspace::WindowKey,
    outcome: &'static str,
    detail: Option<&'static str>,
    project_topmost: bool,
    counter: &mut usize,
) {
    if *counter >= MAX_HIDE_DIAG_PER_SELECT {
        return;
    }
    let token = state.member_tokens.get(key).cloned().unwrap_or_default();
    let elapsed_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    log_json_at(
        &state.log_path.clone(),
        hide_log_line(
            state.tick,
            &ctx.correlation,
            if token.is_empty() {
                None
            } else {
                Some(token.as_str())
            },
            outcome,
            detail,
            project_topmost,
            elapsed_ms,
        ),
    );
    *counter += 1;
}

/// Bounded per-window reveal diagnostic through the shared per-select cap.
/// Silent past the cap; the caller reports the remainder through
/// [`omission_summary`].
pub(super) fn note_reveal_diag(
    state: &mut TileLoop,
    ctx: &ActionCtx,
    key: &crate::workspace::WindowKey,
    outcome: &'static str,
    detail: Option<&'static str>,
    phase: Option<&'static str>,
    counter: &mut usize,
) {
    if *counter >= REVEAL_DIAG_PER_SELECT {
        return;
    }
    let token = state.member_tokens.get(key).cloned().unwrap_or_default();
    let elapsed_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    log_json_at(
        &state.log_path.clone(),
        reveal_log_line(
            state.tick,
            &ctx.correlation,
            if token.is_empty() {
                None
            } else {
                Some(token.as_str())
            },
            outcome,
            detail,
            phase,
            elapsed_ms,
        ),
    );
    *counter += 1;
}

/// Bounded phase summary for one capped per-window diagnostic leg. `None`
/// when every attempt logged; otherwise the honest attempted/logged/omitted
/// counts under the action correlation.
pub(super) fn omission_summary(
    event: &'static str,
    tick: u64,
    correlation: &str,
    phase: Option<&'static str>,
    attempted: usize,
    logged: usize,
) -> Option<serde_json::Value> {
    if logged >= attempted {
        return None;
    }
    Some(serde_json::json!({
        "event": event,
        "tick": tick,
        "correlation": correlation,
        "phase": phase.map_or(serde_json::Value::Null, serde_json::Value::from),
        "attempted": attempted,
        "logged": logged,
        "omitted": attempted - logged,
    }))
}

/// Distinguish one floating-gesture refusal's predicate origin.
pub(super) fn gesture_float_cause(engine_float: bool, workspace_tiled: bool) -> &'static str {
    match (engine_float, workspace_tiled) {
        (true, false) => "engine-exception+workspace-floating",
        (true, true) => "engine-exception",
        (false, false) => "workspace-floating",
        (false, true) => "none",
    }
}

#[cfg(test)]
thread_local! {
    static INJECTED_VIS: std::cell::RefCell<HashMap<u64, (bool, Option<bool>)>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Script one identity-fenced visibility read for `hwnd` (test-only):
/// `(visible, cloaked)`; `None` cloak stands in for an unreadable cloak
/// attribute. Reads only after the lifetime fence passes, exactly like
/// production. Overwrites any previous injection; [`clear_visibility`] resets.
#[cfg(test)]
pub(super) fn inject_visibility(hwnd: u64, visible: bool, cloaked: Option<bool>) {
    INJECTED_VIS.with(|map| {
        map.borrow_mut().insert(hwnd, (visible, cloaked));
    });
}

/// Drop every injected visibility read (test-only).
#[cfg(test)]
pub(super) fn clear_visibility() {
    INJECTED_VIS.with(|map| {
        map.borrow_mut().clear();
    });
}

/// Identity-fenced read-only visibility probe for one managed member lacking
/// fresh observed/retained facts. The stored member-lifetime tag must verify
/// against the live tag first (same bar as the hide effects); any fence
/// mismatch or unreadable read reports unknown. Wait-free reads only, never
/// a message round-trip, never a write.
fn fenced_visibility(
    key: &crate::workspace::WindowKey,
    stored_tag: &str,
) -> (Option<bool>, Option<bool>) {
    if stored_tag.is_empty() {
        return (None, None);
    }
    if !crate::workspace_owner::visible_lifetime_ok(
        stored_tag,
        live_member_tag(key.hwnd).as_deref(),
    ) {
        return (None, None);
    }
    #[cfg(test)]
    if let Some(probe) = INJECTED_VIS.with(|map| map.borrow().get(&key.hwnd).copied()) {
        return (Some(probe.0), probe.1);
    }
    let visible = crate::ffi::is_visible(key.hwnd);
    let cloaked = crate::ffi::cloaked(key.hwnd);
    (Some(visible), cloaked)
}

fn overlay_of(observed: Option<&ObservedWindow>, retained: Option<&RetainedRow>) -> &'static str {
    if let Some(window) = observed {
        if window.facts.captionless_fullscreen {
            "fullscreen"
        } else if window.facts.maximized {
            "maximized"
        } else {
            "none"
        }
    } else if let Some(row) = retained {
        if row.fullscreen {
            "fullscreen"
        } else if row.maximized {
            "maximized"
        } else {
            "none"
        }
    } else {
        "unknown"
    }
}

fn absent_for(skip: Option<&str>) -> (&'static str, &'static str) {
    match skip {
        Some("minimized") => ("absent-minimized", "minimized"),
        Some("unreadable") => ("absent-unreadable", "unreadable"),
        Some("cloaked") => ("absent-cloaked", "native-cloaked"),
        Some("scope-excluded") => ("absent-scope", "scope-excluded"),
        _ => ("absent", "not-observed"),
    }
}

/// Stage one visibility snapshot over the structures `observe` already
/// produced. Members with fresh observed/retained facts reuse them with no
/// native read; hidden claims, frameless retained rows, and absent members
/// take one identity-fenced read-only probe each. Prunes by live membership.
pub(super) fn stage_visibility(
    state: &mut TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    skipped: &[(String, String)],
) {
    let tick = state.tick;
    let staged_at = log_mono_ms();
    let mut observed_by_token: HashMap<&str, &ObservedWindow> = HashMap::new();
    for window in observed {
        observed_by_token.insert(window.token.as_str(), window);
    }
    let mut retained_by_token: HashMap<&str, &RetainedRow> = HashMap::new();
    for row in retained {
        retained_by_token.insert(row.token.as_str(), row);
    }
    let mut skip_reason: HashMap<&str, &str> = HashMap::new();
    for (token, reason) in skipped {
        skip_reason.insert(token.as_str(), reason.as_str());
    }
    for (key, token) in state.member_tokens.clone().iter() {
        let hidden = state.hidden_claims.contains_key(key);
        let observed_window = observed_by_token.get(token.as_str()).copied();
        let retained_row = retained_by_token.get(token.as_str()).copied();
        let overlay = overlay_of(observed_window, retained_row);
        let (vis_state, reason, visible, cloaked) = if hidden {
            let stored = state.member_tags.get(key).cloned().unwrap_or_default();
            let (visible, cloaked) = fenced_visibility(key, &stored);
            ("hidden", "workspace-hide-claim", visible, cloaked)
        } else if let Some(window) = observed_window {
            (
                if window.facts.cloaked {
                    "observed-cloaked"
                } else {
                    "observed"
                },
                if window.facts.cloaked {
                    "native-cloaked"
                } else {
                    "eligible"
                },
                Some(true),
                Some(window.facts.cloaked),
            )
        } else if let Some(row) = retained_row
            && row.facts.is_some()
        {
            let facts = row.facts.expect("gated above");
            (
                if row.fullscreen {
                    "retained-fullscreen"
                } else if row.maximized {
                    "retained-maximized"
                } else if facts.cloaked {
                    "retained-cloaked"
                } else {
                    "retained-other"
                },
                if row.fullscreen || row.maximized {
                    "overlay"
                } else if facts.cloaked {
                    "native-cloaked"
                } else {
                    "ineligible"
                },
                Some(true),
                Some(facts.cloaked),
            )
        } else {
            let stored = state.member_tags.get(key).cloned().unwrap_or_default();
            let (visible, cloaked) = fenced_visibility(key, &stored);
            let (vis_state, reason) = absent_for(skip_reason.get(token.as_str()).copied());
            (vis_state, reason, visible, cloaked)
        };
        let sample = VisibilitySample {
            state: vis_state,
            reason,
            visible,
            cloaked,
            hide_claim: hidden,
            overlay,
            tick,
            observed_ms: staged_at,
        };
        let diag = &mut state.vis_diag;
        if let Some(prev) = diag.vis_pending.get(token)
            && !prev.stable_eq(&sample)
            && diag
                .vis_emitted
                .get(token)
                .is_none_or(|base| !prev.stable_eq(base))
        {
            diag.vis_lost.insert(token.clone());
        }
        diag.vis_pending.insert(token.clone(), sample);
    }
    let diag = &mut state.vis_diag;
    let live: HashSet<String> = state.member_tokens.values().cloned().collect();
    diag.vis_pending.retain(|token, _| live.contains(token));
    diag.vis_emitted.retain(|token, _| live.contains(token));
    diag.vis_emit_seq.retain(|token, _| live.contains(token));
    diag.vis_coalesced.retain(|token| live.contains(token));
    diag.vis_lost.retain(|token| live.contains(token));
}

/// Emit one batched `visibility-transition` event for samples that changed
/// since the last emission, plus tokens whose intermediate history was lost
/// to coalescing even when the latest sample matches the baseline (marked
/// `history_lost`, never silently dropped). Steady ticks emit nothing;
/// throttled rounds merge tokens into `coalesced` and entries past the cap
/// stay pending with an honest `omitted` count. Fair order: never-emitted
/// and oldest-emitted first.
pub(super) fn flush_visibility(state: &mut TileLoop) -> Option<serde_json::Value> {
    let tick = state.tick;
    let now = hint_now();
    let diag = &mut state.vis_diag;
    let mut changed: Vec<(u64, String)> = Vec::new();
    for (token, sample) in diag.vis_pending.iter() {
        let baseline = diag.vis_emitted.get(token);
        if baseline.is_some_and(|baseline| sample.stable_eq(baseline))
            && !diag.vis_lost.contains(token)
        {
            continue;
        }
        let order = diag.vis_emit_seq.get(token).copied().unwrap_or(0);
        changed.push((order, token.clone()));
    }
    changed.sort();
    if changed.is_empty() {
        return None;
    }
    if diag
        .vis_last_emit
        .is_some_and(|at| now.saturating_duration_since(at) < VIS_DIAG_INTERVAL)
    {
        diag.vis_coalesced
            .extend(changed.iter().map(|(_, token)| token.clone()));
        return None;
    }
    let take = changed.len().min(VIS_DIAG_MAX_ENTRIES);
    let mut entries = Vec::with_capacity(take);
    for (_, token) in &changed[..take] {
        let sample = diag.vis_pending.get(token).cloned().expect("changed token");
        let history_lost = diag.vis_lost.remove(token);
        entries.push(serde_json::json!({
            "window": token,
            "state": sample.state,
            "reason": sample.reason,
            "tick": sample.tick,
            "observed_ms": sample.observed_ms,
            "visible": sample.visible,
            "cloaked": sample.cloaked,
            "hide_claim": sample.hide_claim,
            "overlay": sample.overlay,
            "history_lost": history_lost,
        }));
        diag.vis_emitted.insert(token.clone(), sample);
        diag.vis_emit_clock += 1;
        diag.vis_emit_seq.insert(token.clone(), diag.vis_emit_clock);
    }
    let omitted = changed.len() - take;
    let coalesced: Vec<String> = std::mem::take(&mut diag.vis_coalesced)
        .into_iter()
        .collect();
    diag.vis_last_emit = Some(now);
    Some(serde_json::json!({
        "event": "visibility-transition",
        "tick": tick,
        "entries": entries,
        "omitted": omitted,
        "coalesced": coalesced.len(),
    }))
}

/// Current exception mask for one member: adapter lifetime lanes plus the
/// live Engine exception bit for its workspace domain. Pure read.
fn exc_mask_for(state: &TileLoop, key: &crate::workspace::WindowKey, token: &str) -> u8 {
    let mut mask = 0u8;
    if state.floated.contains(key) {
        mask |= LANE_FLOAT;
    }
    if state.sticky.contains_key(key) {
        mask |= LANE_STICKY;
    }
    if state.born_fullscreen.contains(key) {
        mask |= LANE_BORN;
    }
    if state.fixed_auto_seen.contains(key) {
        mask |= LANE_AUTO_FIXED;
    }
    if let Some(loc) = state.workspaces.member_loc(key)
        && engine_is_float(state, &loc.output, &loc.workspace, token)
    {
        mask |= LANE_ENGINE;
    }
    mask
}

/// Stage one exception-lane snapshot per live member. Fully tiled first
/// sightings record silently; any nonzero first sighting or mask change
/// stages a pending sample. Prunes by live membership.
pub(super) fn stage_exceptions(state: &mut TileLoop) {
    let tick = state.tick;
    let staged_at = log_mono_ms();
    for (key, token) in state.member_tokens.clone().iter() {
        let mask = exc_mask_for(state, key, token);
        let diag = &mut state.vis_diag;
        match diag.exc_masks.get(token) {
            None => {
                diag.exc_masks.insert(token.clone(), mask);
                if mask != 0 {
                    diag.exc_pending.insert(
                        token.clone(),
                        ExceptionSample {
                            mask,
                            tick,
                            observed_ms: staged_at,
                        },
                    );
                }
            }
            Some(prior) if *prior != mask => {
                if let Some(prev) = diag.exc_pending.get(token)
                    && prev.mask != mask
                    && diag
                        .exc_emitted
                        .get(token)
                        .is_none_or(|base| prev.mask != *base)
                {
                    diag.exc_lost.insert(token.clone());
                }
                diag.exc_masks.insert(token.clone(), mask);
                diag.exc_pending.insert(
                    token.clone(),
                    ExceptionSample {
                        mask,
                        tick,
                        observed_ms: staged_at,
                    },
                );
            }
            _ => {}
        }
    }
    let diag = &mut state.vis_diag;
    let live: HashSet<String> = state.member_tokens.values().cloned().collect();
    diag.exc_masks.retain(|token, _| live.contains(token));
    diag.exc_pending.retain(|token, _| live.contains(token));
    diag.exc_emitted.retain(|token, _| live.contains(token));
    diag.exc_emit_seq.retain(|token, _| live.contains(token));
    diag.exc_coalesced.retain(|token| live.contains(token));
    diag.exc_lost.retain(|token| live.contains(token));
}

/// Emit one batched `exception-transition` event with the same
/// change/lost-history/throttle/cap contract as [`flush_visibility`].
pub(super) fn flush_exceptions(state: &mut TileLoop) -> Option<serde_json::Value> {
    let tick = state.tick;
    let now = hint_now();
    let diag = &mut state.vis_diag;
    let mut changed: Vec<(u64, String)> = Vec::new();
    for (token, sample) in diag.exc_pending.iter() {
        let baseline = diag.exc_emitted.get(token).copied().unwrap_or(0);
        if baseline == sample.mask && !diag.exc_lost.contains(token) {
            continue;
        }
        let order = diag.exc_emit_seq.get(token).copied().unwrap_or(0);
        changed.push((order, token.clone()));
    }
    changed.sort();
    if changed.is_empty() {
        return None;
    }
    if diag
        .exc_last_emit
        .is_some_and(|at| now.saturating_duration_since(at) < EXC_DIAG_INTERVAL)
    {
        diag.exc_coalesced
            .extend(changed.iter().map(|(_, token)| token.clone()));
        return None;
    }
    let take = changed.len().min(EXC_DIAG_MAX_ENTRIES);
    let mut entries = Vec::with_capacity(take);
    for (_, token) in &changed[..take] {
        let sample = diag.exc_pending.get(token).cloned().expect("changed token");
        let baseline = diag.exc_emitted.get(token).copied().unwrap_or(0);
        let history_lost = diag.exc_lost.remove(token);
        let transition = if sample.mask == baseline {
            "unchanged"
        } else if baseline == 0 {
            "entered"
        } else if sample.mask == 0 {
            "exited"
        } else {
            "changed"
        };
        entries.push(serde_json::json!({
            "window": token,
            "lanes": lane_names(sample.mask),
            "transition": transition,
            "reason": exc_reason(sample.mask),
            "tick": sample.tick,
            "observed_ms": sample.observed_ms,
            "history_lost": history_lost,
        }));
        diag.exc_emitted.insert(token.clone(), sample.mask);
        diag.exc_emit_clock += 1;
        diag.exc_emit_seq.insert(token.clone(), diag.exc_emit_clock);
    }
    let omitted = changed.len() - take;
    let coalesced: Vec<String> = std::mem::take(&mut diag.exc_coalesced)
        .into_iter()
        .collect();
    diag.exc_last_emit = Some(now);
    Some(serde_json::json!({
        "event": "exception-transition",
        "tick": tick,
        "entries": entries,
        "omitted": omitted,
        "coalesced": coalesced.len(),
    }))
}

/// Narrow rollback shared by every scoped transition: reveal exactly the
/// claims committed above (exact identity, no blind replay, no fabricated
/// rollback) through one joint reveal phase with per-window
/// `workspace-reveal` lines in the `rollback` phase plus a bounded omission
/// summary. Retired claims drop member state; revealed claims unflag;
/// failures keep the ledger for the next attempt.
pub(super) fn rollback_newly_hidden(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    newly_hidden: &[crate::workspace::WindowKey],
    ctx: &ActionCtx,
    clock: &mut impl crate::product_hide::batch::BatchWaitClock,
) {
    let reveal = reveal_phase(
        state,
        me,
        store,
        ctx,
        newly_hidden,
        Some("rollback"),
        true,
        clock,
    );
    if let Some(line) = omission_summary(
        "workspace-reveal-summary",
        state.tick,
        &ctx.correlation,
        Some("rollback"),
        reveal.attempted,
        reveal.logged,
    ) {
        let t0 = Instant::now();
        log_json_at(&state.log_path.clone(), line);
        let _ = t0;
    }
    log_visibility_timing(
        state,
        ctx,
        "rollback",
        &PhaseBreakdown::default(),
        &reveal.breakdown,
    );
}

/// Drop every visibility/exception sample for one dead member.
pub(super) fn drop_visibility_sample(state: &mut TileLoop, token: &str) {
    let diag = &mut state.vis_diag;
    diag.vis_pending.remove(token);
    diag.vis_emitted.remove(token);
    diag.vis_emit_seq.remove(token);
    diag.vis_coalesced.remove(token);
    diag.vis_lost.remove(token);
    diag.exc_masks.remove(token);
    diag.exc_pending.remove(token);
    diag.exc_emitted.remove(token);
    diag.exc_emit_seq.remove(token);
    diag.exc_coalesced.remove(token);
    diag.exc_lost.remove(token);
}
