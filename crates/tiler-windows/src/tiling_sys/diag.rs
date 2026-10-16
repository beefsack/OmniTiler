use super::*;

/// Bounded normal-level per-window diagnostics for hint/geometry flaps.
///
/// One batched `window-diag` event joins, per opaque member token, what the
/// old normal log left scattered or untraceable: the window class plus
/// executable basename (never titles, full paths, SIDs, or content), the
/// fresh hint query outcome with the actual native cost (offered timeout
/// recomputed at the call plus the measured native round-trip, both zero
/// when no query was attempted), the carried min/max, fallback use plus age
/// and explicit reason, the Engine-planned vs effective requested visible
/// rectangle, the native outer target when a setter ran, native pre/post
/// readback rectangles, the write disposition, and the [`RefusedTracker`]
/// suppression/re-arm reason. A successful setter alone proves nothing here:
/// only the readback comparison speaks.
///
/// Batching contract: at most one event per second globally, at most
/// [`WINDOW_DIAG_MAX_ENTRIES`] entries per event. Every entry carries its
/// own operation/correlation/tick context from the assembly that staged it,
/// so one batch may mix domains while each entry still joins correctly; the
/// envelope carries no operation/correlation of its own. Entries past the cap
/// stay pending with an honest `omitted` count, and throttled rounds merge
/// into the `coalesced` token set (unique tokens, never double-counted).
/// Emission rotates by per-token emission order (never-emitted and
/// oldest-emitted first), so a continuously flapping head can never starve
/// the tail: every live pending member, including recoveries, eventually
/// emits. Steady ticks emit nothing: change dedupe compares semantic facts
/// only and ignores advisory timing (fallback age, offered timeout, query
/// elapsed) plus labels (operation, correlation, ticks).
///
/// Lifetime contract: the latest completed sample per token survives
/// throttling and off-domain ticks (a sample staged by assembly and
/// completed by application stays eligible until it emits, even ticks
/// later); memory prunes by live member lifetimes alongside the existing
/// hint caches. Class names resolve once per member lifetime and only for
/// entries that actually emit; an identity break marks the class unknown
/// until a performed query re-binds, so a recycled HWND never reports the
/// old window's class. No logging timer, no extra native query on quiet
/// samples, and no wait or repeated identity open anywhere on this path.
/// External apps recycle HWNDs concurrently with the loop thread, so a HWND
/// staged earlier may name a different window by emission time. The class
/// read is therefore fenced at emission (stored pid plus lifetime tag
/// verified immediately before and after, unknown on any mismatch or
/// unverifiable read), and an identity break proven by the hint lane marks
/// the class unknown until a performed query re-binds. The tag/pid/creation
/// fences still guard every write separately.
pub(super) const WINDOW_DIAG_MAX_ENTRIES: usize = 16;
pub(super) const WINDOW_DIAG_INTERVAL: Duration = Duration::from_secs(1);

/// One per-window sample; hint-lane fields stage in row assembly, write-lane
/// fields stage in geometry application. A sample is flush-eligible once
/// both lanes staged it (`write_tick != 0`), on any later tick, without
/// relabeling: the stored operation/correlation/tick always describe the
/// assembly that produced it. `fallback_age_ms`, `timeout_ms`, `query_us`,
/// `op`, `correlation`, `hint_tick`, and `write_tick` never participate in
/// change dedupe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WindowDiagSample {
    pub(super) output: String,
    pub(super) workspace: String,
    pub(super) op: String,
    pub(super) correlation: String,
    pub(super) hwnd: u64,
    pub(super) exe: String,
    pub(super) outcome: &'static str,
    pub(super) timeout_ms: u32,
    pub(super) query_us: u64,
    pub(super) carried: WindowSizeHints,
    pub(super) fallback: bool,
    pub(super) fallback_age_ms: u64,
    pub(super) fallback_reason: &'static str,
    pub(super) observed_outer: Option<Rect>,
    pub(super) insets: [i32; 4],
    pub(super) planned: Option<Rect>,
    pub(super) effective: Option<Rect>,
    pub(super) outer: Option<Rect>,
    pub(super) pre: Option<Rect>,
    pub(super) post: Option<Rect>,
    pub(super) disposition: &'static str,
    pub(super) suppress: &'static str,
    pub(super) hint_tick: u64,
    pub(super) write_tick: u64,
}

impl WindowDiagSample {
    fn blank(output: &str, workspace: &str, op: &str, correlation: &str) -> Self {
        Self {
            output: output.to_owned(),
            workspace: workspace.to_owned(),
            op: op.to_owned(),
            correlation: correlation.to_owned(),
            hwnd: 0,
            exe: String::new(),
            outcome: "retained",
            timeout_ms: 0,
            query_us: 0,
            carried: WindowSizeHints::none(),
            fallback: false,
            fallback_age_ms: 0,
            fallback_reason: "retained",
            observed_outer: None,
            insets: [0; 4],
            planned: None,
            effective: None,
            outer: None,
            pre: None,
            post: None,
            disposition: "no-write",
            suppress: "none",
            hint_tick: 0,
            write_tick: 0,
        }
    }

    /// Stable-change equality: semantic facts only. Advisory timing
    /// (`fallback_age_ms`, `timeout_ms`, `query_us`) jitters with remaining
    /// budget and scheduling and never re-arms an emission by itself; labels
    /// (`op`, `correlation`, ticks) describe provenance, not state. A
    /// repeated identical flap coalesces; any genuine outcome/geometry/
    /// disposition/identity change re-arms.
    fn stable_eq(&self, other: &Self) -> bool {
        self.output == other.output
            && self.workspace == other.workspace
            && self.exe == other.exe
            && self.outcome == other.outcome
            && self.carried == other.carried
            && self.fallback == other.fallback
            && self.fallback_reason == other.fallback_reason
            && self.observed_outer == other.observed_outer
            && self.insets == other.insets
            && self.planned == other.planned
            && self.effective == other.effective
            && self.outer == other.outer
            && self.pre == other.pre
            && self.post == other.post
            && self.disposition == other.disposition
            && self.suppress == other.suppress
    }

    /// Whether this sample is incident evidence rather than steady state: an
    /// unknown hint lane, a fallback ride, any non-quiet write disposition,
    /// or a readback that disagrees with the request. A converged `wrote`
    /// (post equals effective) stays quiet; its clearing `match` flushes the
    /// recovery only when a problem was previously emitted.
    fn is_problem(&self) -> bool {
        if !matches!(self.outcome, "hint" | "retained" | "retained-hint") {
            return true;
        }
        if self.fallback {
            return true;
        }
        match self.disposition {
            "match" | "retained" | "no-write" | "no-plan" => {}
            "wrote" => {
                if self.post != self.effective {
                    return true;
                }
            }
            _ => return true,
        }
        if self.post.is_some_and(|post| Some(post) != self.effective) {
            return true;
        }
        false
    }
}

/// Map one apply skip reason onto the closed diagnostic disposition
/// vocabulary (existing log tokens verbatim); anything dynamic or
/// unrecognized buckets honestly as `skipped` rather than inventing detail.
fn static_disposition(reason: &str) -> &'static str {
    match reason {
        "client-clamped" => "client-clamped",
        "retained" => "retained",
        "vanished" => "vanished",
        "refused-intent" => "refused-intent",
        "frame-overflow" => "frame-overflow",
        "fullscreen-foreground" => "fullscreen-foreground",
        "pid-changed" => "pid-changed",
        "setter-failed" => "setter-failed",
        "identity-changed" => "identity-changed",
        "integrity-changed" => "integrity-changed",
        "ineligible" => "ineligible",
        "scope-excluded" => "scope-excluded",
        "hidden" => "hidden",
        "minimized" => "minimized",
        "maximized" => "maximized",
        "cloaked" => "cloaked",
        "elevated" => "elevated",
        "shell" => "shell",
        "tool" => "tool",
        "owned-dialog" => "owned-dialog",
        "dialog" => "dialog",
        "fullscreen" => "fullscreen",
        "no-activate" => "no-activate",
        "unreadable" => "unreadable",
        _ => "skipped",
    }
}

/// Suppression/re-arm reason for one staged sample from the pre-write
/// [`RefusedTracker`] snapshot: the in-loop skip decision recomputed
/// faithfully for `refused-intent` (identical clamp pair vs transient
/// backoff), and lane transitions once a write proceeds (desired or observed
/// genuinely changed, a converged post clearing a held transient pair as
/// `exact-match`, or a lapsed transient backoff). Pure read; tracker
/// behavior untouched.
fn suppress_reason(
    snapshot: &RefusedTracker,
    token: &str,
    disposition: &'static str,
    effective: Option<Rect>,
    pre: Option<Rect>,
    post: Option<Rect>,
    now: Instant,
) -> &'static str {
    let (Some(effective), Some(pre)) = (effective, pre) else {
        return "none";
    };
    if disposition == "refused-intent" {
        if snapshot
            .clamp_pair(token)
            .is_some_and(|(d, o)| d == effective && o == pre)
        {
            return "identical-clamp";
        }
        if snapshot.should_skip(token, &effective, &pre, now) {
            return "transient-backoff";
        }
        return "transient-expired";
    }
    if let Some((d, o)) = snapshot.clamp_pair(token) {
        // A held clamp pair alongside a proceeding write moved on at least
        // one side: an identical pair always suppresses, and equal pairs are
        // never recorded (`note_clamp` clears those), so report which side
        // re-armed the lane. The trailing arm is defensive and unreachable.
        if d != effective {
            return "desired-changed";
        }
        if o != pre {
            return "observed-changed";
        }
        return "identical-clamp";
    }
    if let Some((d, o)) = snapshot.transient_pair(token) {
        if d != effective {
            return "desired-changed";
        }
        if o != pre {
            return "observed-changed";
        }
        if post.is_some_and(|post| post == effective) {
            return "exact-match";
        }
        return "transient-expired";
    }
    "none"
}

/// Stage the hint lane for every fresh-query row assembly just produced:
/// outcome, actual native cost, carried hints, and fallback facts from
/// the per-token report (fresh lanes only; retained rows carry the cached
/// overlay value with no query and no fallback). The sample records this
/// assembly's operation/correlation for per-entry joins. Observed geometry
/// and the executable basename come from the already-observed member paths
/// only: no new native query. Resets the write lane (recompleted by
/// application the same tick) and prunes all diagnostic maps by live
/// membership.
#[allow(clippy::too_many_arguments)]
pub(super) fn stage_hint_samples(
    state: &mut TileLoop,
    output_token: &str,
    workspace_token: &str,
    op: &str,
    correlation: &str,
    rows: &[crate::workspace_owner::OwnerRow],
    reasons: &HashMap<String, &'static str>,
    costs: &HashMap<String, QueryCost>,
    observed: &[ObservedWindow],
    query_ms: u64,
) {
    let tick = state.tick;
    let by_token: HashMap<&str, &ObservedWindow> =
        observed.iter().map(|w| (w.token.as_str(), w)).collect();
    let mut hwnd_by_token: HashMap<&str, u64> = observed
        .iter()
        .map(|w| (w.token.as_str(), w.hwnd))
        .collect();
    // Stored identity by live token in one pass (no per-row scan): the
    // basename fallback for rows with no fresh observation.
    let mut ident_by_token: HashMap<&str, &ProcessIdentity> = HashMap::new();
    for (key, token) in state.member_tokens.iter() {
        hwnd_by_token.entry(token.as_str()).or_insert(key.hwnd);
        if let Some(ident) = state.member_identity.get(key) {
            ident_by_token.insert(token.as_str(), ident);
        }
    }
    for row in rows {
        let Some(outcome) = reasons.get(&row.token).copied() else {
            // No query ran on this lane (slotless floats ride the live
            // frame with no hint): nothing diagnostic to stage.
            continue;
        };
        let fresh_lane = matches!(
            outcome,
            "hint" | "no-hint" | "timeout" | "failed" | "invalid" | "budget"
        );
        let (fallback, age_ms, fallback_reason) = if fresh_lane {
            match state.hint_fallback_uses.get(&row.token) {
                Some(report) => (report.reused, report.age_ms, report.reason),
                None => (false, 0, outcome),
            }
        } else {
            (false, 0, outcome)
        };
        let exe = by_token
            .get(row.token.as_str())
            .map(|w| exe_file_name(&w.identity.exe_path))
            .or_else(|| {
                ident_by_token
                    .get(row.token.as_str())
                    .map(|ident| exe_file_name(&ident.exe_path))
            })
            .unwrap_or_default()
            .to_owned();
        let cost = costs.get(&row.token).copied().unwrap_or_default();
        let sample = state
            .diag_pending
            .entry(row.token.clone())
            .or_insert_with(|| {
                WindowDiagSample::blank(output_token, workspace_token, op, correlation)
            });
        sample.output = output_token.to_owned();
        sample.workspace = workspace_token.to_owned();
        sample.op = op.to_owned();
        sample.correlation = correlation.to_owned();
        sample.hwnd = hwnd_by_token.get(row.token.as_str()).copied().unwrap_or(0);
        sample.exe = exe;
        sample.outcome = outcome;
        sample.timeout_ms = cost.offered_ms;
        sample.query_us = cost.elapsed_us;
        sample.carried = row.hints;
        sample.fallback = fallback;
        sample.fallback_age_ms = age_ms;
        sample.fallback_reason = fallback_reason;
        if let Some(window) = by_token.get(row.token.as_str()) {
            sample.observed_outer = Some(window.outer);
            sample.insets = [
                window.insets.left,
                window.insets.top,
                window.insets.right,
                window.insets.bottom,
            ];
        } else {
            sample.observed_outer = None;
            sample.insets = [0; 4];
        }
        sample.pre = Some(row.rect);
        sample.planned = None;
        sample.effective = None;
        sample.outer = None;
        sample.post = None;
        sample.disposition = "no-write";
        sample.suppress = "none";
        sample.hint_tick = tick;
        sample.write_tick = 0;
    }
    state.diag_query_ms = query_ms;
    let live: HashSet<String> = state.member_tokens.values().cloned().collect();
    state.diag_pending.retain(|token, _| live.contains(token));
    state.diag_emitted.retain(|token, _| live.contains(token));
    state.diag_class.retain(|token, _| live.contains(token));
    state.diag_emit_seq.retain(|token, _| live.contains(token));
    state.diag_coalesced.retain(|token| live.contains(token));
}

/// Stage the write lane for one geometry application pass over the structures
/// the pass already produced: desired entries, the skip list (last wins),
/// attempted setters, native outer targets, pre-write observations, and
/// readback rectangles. Tokens with a hint sample but no desired entry
/// complete honestly as `no-write` (floats and unplanned rows take no
/// writes). Called by the production apply path and by tests through the
/// same seam; never queries natively.
#[allow(clippy::too_many_arguments)]
pub(super) fn stage_write_samples(
    state: &mut TileLoop,
    desired: &[crate::workspace_owner::PlannedWrite],
    skipped: &[(String, String)],
    written: &HashSet<String>,
    outer_targets: &HashMap<String, Rect>,
    observed: &[ObservedWindow],
    post_rects: &HashMap<String, Rect>,
    snapshot: &RefusedTracker,
    now: Instant,
) {
    let tick = state.tick;
    let pre_by_token: HashMap<&str, Rect> = observed
        .iter()
        .map(|w| (w.token.as_str(), w.visible))
        .collect();
    let mut last_skip: HashMap<&str, &str> = HashMap::new();
    for (token, reason) in skipped {
        last_skip.insert(token.as_str(), reason.as_str());
    }
    for entry in desired {
        let token = entry.window.0.as_str();
        let Some(sample) = state.diag_pending.get_mut(token) else {
            continue;
        };
        if sample.hint_tick != tick {
            continue;
        }
        let hints = state
            .hint_logged
            .get(token)
            .copied()
            .unwrap_or(WindowSizeHints::none());
        let effective = if entry.overconstrained {
            crate::tiling::overconstrained_effective(entry.rect, hints)
        } else {
            entry.rect
        };
        let disposition = match last_skip.get(token) {
            Some(reason) => static_disposition(reason),
            None if written.contains(token) => "wrote",
            None => "match",
        };
        let pre = pre_by_token.get(token).copied().or(sample.pre);
        let post = post_rects.get(token).copied();
        sample.planned = Some(entry.rect);
        sample.effective = Some(effective);
        sample.outer = outer_targets.get(token).copied();
        sample.pre = pre;
        sample.post = post;
        sample.disposition = disposition;
        sample.suppress = suppress_reason(
            snapshot,
            token,
            disposition,
            Some(effective),
            pre,
            post,
            now,
        );
        sample.write_tick = tick;
    }
    for sample in state.diag_pending.values_mut() {
        if sample.hint_tick == tick && sample.write_tick != tick {
            sample.disposition = "no-write";
            sample.suppress = "none";
            sample.write_tick = tick;
        }
    }
}

/// Complete hint-staged samples when the Engine reply carries no plan
/// (rejected/diverged/invalid): no write lane exists, and the samples must
/// still flush rather than linger half-staged.
pub(super) fn stage_no_plan(state: &mut TileLoop) {
    let tick = state.tick;
    for sample in state.diag_pending.values_mut() {
        if sample.hint_tick == tick && sample.write_tick != tick {
            sample.disposition = "no-plan";
            sample.suppress = "none";
            sample.write_tick = tick;
        }
    }
}

/// Drop every diagnostic sample for one dead member. Mirrors the existing
/// hint-cache retirement sites.
pub(super) fn drop_diag_sample(state: &mut TileLoop, token: &str) {
    state.diag_pending.remove(token);
    state.diag_emitted.remove(token);
    state.diag_class.remove(token);
    state.diag_emit_seq.remove(token);
    state.diag_coalesced.remove(token);
}

/// Identity break for one token: the live member no longer matches the
/// lifecycle the diagnostics were bound to (live lifetime-tag mismatch or a
/// token/tag lifecycle move that just dropped the entry). Drops the
/// published baseline (it describes a stale window and is not comparable)
/// and marks the class unknown (empty, never re-read while marked) so a
/// recycled HWND never reports the old window's class. The fresh fallback
/// report just written is untouched. A later performed query that proves the
/// live identity trustworthy re-binds (see [`diag_identity_rebound`]).
pub(super) fn diag_identity_broke(state: &mut TileLoop, token: &str) {
    state.diag_emitted.remove(token);
    state.diag_class.insert(token.to_owned(), String::new());
}

/// Re-bind after a performed query proved the live identity trustworthy
/// (`Hint`, `NoHint`, or a lifetime-verified Timeout reuse): drops only the
/// unknown mark so the next emission re-resolves the class live. Trusted
/// cached classes are never dropped here, so successful queries never cause
/// class re-reads by themselves.
pub(super) fn diag_identity_rebound(state: &mut TileLoop, token: &str) {
    if state.diag_class.get(token).is_some_and(String::is_empty) {
        state.diag_class.remove(token);
    }
}

fn rect_json(rect: &Rect) -> serde_json::Value {
    serde_json::json!([rect.x, rect.y, rect.w, rect.h])
}

fn opt_rect_json(rect: &Option<Rect>) -> serde_json::Value {
    rect.as_ref().map_or(serde_json::Value::Null, rect_json)
}

fn opt_bound_json(bound: &Option<i32>) -> serde_json::Value {
    bound.map_or(serde_json::Value::Null, |v| serde_json::json!(v))
}

/// Resolve the window class for one emitted token, once per member lifetime:
/// cached on first emission, never re-read, never read for quiet (unemitted)
/// samples. An identity-break unknown mark (empty cache entry) reports
/// unknown with no read at all until a trustworthy query re-binds.
/// Production reads the live class; the call runs only here.
fn resolve_class(state: &mut TileLoop, token: &str, hwnd: u64) -> String {
    if let Some(cached) = state.diag_class.get(token) {
        return cached.clone();
    }
    // Emission-time lifetime fence: the staged HWND may already name a
    // different window, so bind the read to the currently stored member
    // identity (same HWND, stored pid, stored lifetime tag) immediately
    // before AND after. Any mismatch, stale sample HWND, or unverifiable
    // read reports unknown with no caching, so a later tick retries; only a
    // twice-verified read caches. Wait-free reads only, emission-bounded.
    let (key, stored_tag) = match state
        .member_tokens
        .iter()
        .find_map(|(k, t)| (t == token).then(|| k.clone()))
    {
        Some(key) => {
            let tag = state.member_tags.get(&key).cloned().unwrap_or_default();
            (key, tag)
        }
        None => return String::new(),
    };
    if hwnd != key.hwnd || !class_lifetime_ok(&key, &stored_tag) {
        return String::new();
    }
    #[cfg(test)]
    if let Some(injected) = INJECTED_CLASS.with(|map| map.borrow().get(&hwnd).cloned()) {
        if !class_lifetime_ok(&key, &stored_tag) {
            return String::new();
        }
        state.diag_class.insert(token.to_owned(), injected.clone());
        return injected;
    }
    let class = if hwnd == 0 {
        String::new()
    } else {
        observe::class_of(hwnd as isize as HWND)
    };
    if !class_lifetime_ok(&key, &stored_tag) {
        return String::new();
    }
    state.diag_class.insert(token.to_owned(), class.clone());
    class
}

/// Emission-time lifetime check for one stored member: the live window still
/// carries the stored pid and lifetime tag. Wait-free reads only (process id
/// query plus the property tag, never a message round-trip); false on any
/// mismatch or unreadable live state, so callers report unknown.
fn class_lifetime_ok(key: &crate::workspace::WindowKey, stored_tag: &str) -> bool {
    if stored_tag.is_empty() || key.pid == 0 {
        return false;
    }
    if class_fence_pid(key.hwnd) != key.pid {
        return false;
    }
    crate::workspace_owner::visible_lifetime_ok(stored_tag, live_member_tag(key.hwnd).as_deref())
}

/// Live pid for the emission-time class fence (wait-free, no message
/// round-trip). Test seam below stands in for synthetic-HWND fixtures.
fn class_fence_pid(hwnd: u64) -> u32 {
    #[cfg(test)]
    if let Some(pid) = INJECTED_PID.with(|map| map.borrow().get(&hwnd).copied()) {
        return pid;
    }
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd as isize as HWND, &mut pid);
    }
    pid
}

fn entry_json(state: &mut TileLoop, token: &str, sample: &WindowDiagSample) -> serde_json::Value {
    let class = resolve_class(state, token, sample.hwnd);
    serde_json::json!({
        "window": token,
        "class": class,
        "exe": sample.exe,
        "output": sample.output,
        "workspace": sample.workspace,
        "op": sample.op,
        "correlation": sample.correlation,
        "tick": sample.write_tick,
        "outcome": sample.outcome,
        "timeout_ms": sample.timeout_ms,
        "query_us": sample.query_us,
        "carried_min": [opt_bound_json(&sample.carried.min_w), opt_bound_json(&sample.carried.min_h)],
        "carried_max": [opt_bound_json(&sample.carried.max_w), opt_bound_json(&sample.carried.max_h)],
        "fallback": sample.fallback,
        "fallback_age_ms": sample.fallback_age_ms,
        "fallback_reason": sample.fallback_reason,
        "planned": opt_rect_json(&sample.planned),
        "effective": opt_rect_json(&sample.effective),
        "outer": opt_rect_json(&sample.outer),
        "observed_outer": opt_rect_json(&sample.observed_outer),
        "insets": sample.insets,
        "pre": opt_rect_json(&sample.pre),
        "post": opt_rect_json(&sample.post),
        "disposition": sample.disposition,
        "suppress": sample.suppress,
    })
}

/// Emit one batched `window-diag` event for completed samples (both lanes
/// staged, `write_tick != 0`) that changed since the last emission: first
/// problems, flap alternations, and the final recovery. Eligibility never
/// depends on the current tick, so a sample staged and completed on an idle
/// domain still emits ticks later, in its original operation/correlation
/// context (entries are never relabeled to the flush context, and the
/// envelope carries no operation/correlation of its own). Steady samples and
/// healthy-only changes emit nothing; throttled rounds merge their tokens
/// into `coalesced` (unique, never double-counted) and entries past the cap
/// stay pending with an honest `omitted` count. Emission order is fair:
/// never-emitted and oldest-emitted tokens first, so a flapping head cannot
/// starve the tail. Returns the event value so tests assert on the
/// production payload; the caller logs it.
pub(super) fn flush_window_diag(state: &mut TileLoop) -> Option<serde_json::Value> {
    let tick = state.tick;
    let now = hint_now();
    // (last emission order, token): never-emitted sorts first, then oldest.
    let mut changed: Vec<(u64, String)> = Vec::new();
    for (token, sample) in state.diag_pending.iter() {
        if sample.write_tick == 0 {
            continue;
        }
        let baseline = state.diag_emitted.get(token);
        if baseline.is_some_and(|baseline| sample.stable_eq(baseline)) {
            continue;
        }
        if !(sample.is_problem() || baseline.is_some_and(WindowDiagSample::is_problem)) {
            continue;
        }
        let order = state.diag_emit_seq.get(token).copied().unwrap_or(0);
        changed.push((order, token.clone()));
    }
    changed.sort();
    if changed.is_empty() {
        return None;
    }
    if state
        .diag_last_emit
        .is_some_and(|at| now.saturating_duration_since(at) < WINDOW_DIAG_INTERVAL)
    {
        state
            .diag_coalesced
            .extend(changed.iter().map(|(_, token)| token.clone()));
        return None;
    }
    let take = changed.len().min(WINDOW_DIAG_MAX_ENTRIES);
    let mut entries = Vec::with_capacity(take);
    for (_, token) in &changed[..take] {
        let sample = state
            .diag_pending
            .get(token)
            .cloned()
            .expect("changed token");
        entries.push(entry_json(state, token, &sample));
        state.diag_emitted.insert(token.clone(), sample);
        state.diag_emit_clock += 1;
        state
            .diag_emit_seq
            .insert(token.clone(), state.diag_emit_clock);
    }
    let omitted = changed.len() - take;
    let coalesced: Vec<String> = std::mem::take(&mut state.diag_coalesced)
        .into_iter()
        .collect();
    state.diag_last_emit = Some(now);
    Some(serde_json::json!({
        "event": "window-diag",
        "tick": tick,
        "query_ms": state.diag_query_ms,
        "entries": entries,
        "omitted": omitted,
        "coalesced": coalesced.len(),
    }))
}

#[cfg(test)]
thread_local! {
    static INJECTED_CLASS: std::cell::RefCell<std::collections::HashMap<u64, String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static INJECTED_PID: std::cell::RefCell<std::collections::HashMap<u64, u32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Inject one controlled window class for `hwnd` (test-only): stands in for
/// the native class read with no OS call. Overwrites any previous injection;
/// [`clear_diag_classes`] resets.
#[cfg(test)]
pub(super) fn inject_diag_class(hwnd: u64, class: &str) {
    INJECTED_CLASS.with(|map| {
        map.borrow_mut().insert(hwnd, class.to_owned());
    });
}

/// Drop every injected window class (test-only).
#[cfg(test)]
pub(super) fn clear_diag_classes() {
    INJECTED_CLASS.with(|map| {
        map.borrow_mut().clear();
    });
}

/// Inject one live pid for `hwnd` (test-only): stands in for the native
/// process-id read on the emission-time class fence with no OS call.
/// Overwrites any previous injection; [`clear_diag_pids`] resets.
#[cfg(test)]
pub(super) fn inject_diag_pid(hwnd: u64, pid: u32) {
    INJECTED_PID.with(|map| {
        map.borrow_mut().insert(hwnd, pid);
    });
}

/// Drop every injected live pid (test-only).
#[cfg(test)]
pub(super) fn clear_diag_pids() {
    INJECTED_PID.with(|map| {
        map.borrow_mut().clear();
    });
}

/// Cached emission-time class for `token`, if resolved (test-only reader).
#[cfg(test)]
pub(super) fn diag_class_cached(state: &TileLoop, token: &str) -> Option<String> {
    state.diag_class.get(token).cloned()
}
