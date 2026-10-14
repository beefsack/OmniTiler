//! Bounded Planner protocol (product-shaped, static only).
//!
//! Pure JSON-string-in / JSON-string-out evaluation. Rust owns all policy:
//! the complete normalized current observation (opaque adapter ids, frame
//! rectangles, output, workspace, focus) plus one parameterized command
//! proposes exactly that command through the existing
//! session/reconciler/directional/cosmic_v1 APIs ([`tiler_core::cosmic_v1`]
//! admission axis/shares, no invented tiling semantics).
//!
//! [`Planner`] is the authoritative live-tree route (one
//! committed session per domain across calls; observations validate
//! membership/divergence but never rebuild known topology). Recovery is a
//! single rule: discard and rebuild once on owner/generation, domain, or
//! membership divergence, else reject rather than wedge. Every rejection maps
//! to a bounded recoverable `rejected` outcome (never terminal `diverged`),
//! so a fresh observation can always recover after any rejection. Native
//! execution stays outside: replies carry full target geometries plus
//! retained focus for the adapter to actuate.
//!
//! Workspace send route (synchronous): the `send-to-workspace`
//! operation carries a same-output distinct-workspace target as an optional
//! `target_domain` plus `target_windows` alongside the source `domain`/
//! `windows`. It proposes once through the Engine and commits immediately
//! with native assignment plus both-domain geometry. Legacy requests are
//! unchanged.

use serde::{Deserialize, Serialize};

use tiler_core::bounds::{is_opaque_id, valid_carried_rect};
use tiler_core::contract::{LifecycleOperation, LifecyclePrecondition};
use tiler_core::directional::{Direction, NodeId, OutputId, SameAxisMove, WindowId, WorkspaceId};
use tiler_core::engine::Engine;
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::session::{DomainKey, OutputDomain, RefusalKind};

/// Planner protocol contract version (JSON string v1).
pub const PLAN_CONTRACT_VERSION: u32 = 1;
/// Bounded request cap (1 MiB; mirrors the portable service bound).
pub const PLAN_MAX_REQUEST_BYTES: usize = 1_048_576;
/// Bounded reply cap (mirrors the portable service bound).
pub const PLAN_MAX_REPLY_BYTES: usize = 64 * 1024;
/// Opaque id bound (single source: [`tiler_core::bounds::MAX_OPAQUE_ID_LEN`]).
pub const PLAN_MAX_ID_LEN: usize = tiler_core::bounds::MAX_OPAQUE_ID_LEN;
/// Revision bound (inclusive, shared with contract).
pub const PLAN_MAX_REVISION: u64 = 1_000_000;

const MSG_OVERSIZED: &str = "request exceeds size bound";
const MSG_MALFORMED: &str = "request is malformed";
const MSG_UNKNOWN_FIELD: &str = "request contains an unknown field";
const MSG_UNKNOWN_VALUE: &str = "request contains an unknown value";
const MSG_VERSION: &str = "unsupported contract version";
const MSG_CORRELATION: &str = "correlation id is invalid";
const MSG_OWNER: &str = "owner is invalid";
const MSG_GENERATION: &str = "generation is invalid";
const MSG_REVISION: &str = "revision is invalid";
const MSG_OPAQUE_ID: &str = "opaque id is invalid";
const MSG_OBSERVATION: &str = "observation does not cover the known window set";
const MSG_CROSS_DOMAIN: &str = "input output or workspace does not match a logical domain";
const MSG_DIRECTION: &str = "direction is invalid";

/// Bounded gap extent for carried work-area geometry
/// (single source: [`tiler_core::bounds::MAX_GAP`]).
const GEOMETRY_MAX_GAP: i32 = tiler_core::bounds::MAX_GAP;

/// Canonical production directional fingerprint (FNV-1a 32-bit) over the
/// full two-domain evidence, byte-identical to the adapter's
/// `planDirectionalFingerprint`: ordered domain primitives (output,
/// workspace, raw bounds, gaps, four-direction adjacency), the focused id,
/// every window sorted by id (id, output, workspace, rect, floating,
/// fit-excluded). Any alteration of target rect, bounds, or adjacency
/// changes the value. Legacy single-domain requests keep `planFingerprint`.
fn directional_fingerprint(
    entries: &[DirectionalDomainDto],
    focused: &str,
    windows: &[ObservedDto],
) -> u64 {
    fn feed(hash: &mut u32, text: &str) {
        for byte in text.bytes() {
            *hash ^= u32::from(byte);
            *hash = hash.wrapping_mul(16777619);
        }
    }
    fn sep(hash: &mut u32, byte: u8) {
        *hash ^= u32::from(byte);
        *hash = hash.wrapping_mul(16777619);
    }
    let mut hash: u32 = 2166136261;
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            sep(&mut hash, 0x1e);
        }
        feed(&mut hash, &entry.output);
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.workspace);
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.bounds.x.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.bounds.y.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.bounds.w.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.bounds.h.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.gap.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.outer_gap.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, "left");
        sep(&mut hash, 0x1f);
        feed(
            &mut hash,
            entry.adjacent.get("left").map(String::as_str).unwrap_or(""),
        );
        sep(&mut hash, 0x1f);
        feed(&mut hash, "right");
        sep(&mut hash, 0x1f);
        feed(
            &mut hash,
            entry
                .adjacent
                .get("right")
                .map(String::as_str)
                .unwrap_or(""),
        );
        sep(&mut hash, 0x1f);
        feed(&mut hash, "up");
        sep(&mut hash, 0x1f);
        feed(
            &mut hash,
            entry.adjacent.get("up").map(String::as_str).unwrap_or(""),
        );
        sep(&mut hash, 0x1f);
        feed(&mut hash, "down");
        sep(&mut hash, 0x1f);
        feed(
            &mut hash,
            entry.adjacent.get("down").map(String::as_str).unwrap_or(""),
        );
    }
    sep(&mut hash, 0x1f);
    feed(&mut hash, focused);
    let mut ordered: Vec<&ObservedDto> = windows.iter().collect();
    ordered.sort_by(|a, b| a.window.cmp(&b.window));
    for entry in ordered {
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.window);
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.output);
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.workspace);
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.rect.x.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.rect.y.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.rect.w.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, &entry.rect.h.to_string());
        sep(&mut hash, 0x1f);
        feed(&mut hash, if entry.floating { "1" } else { "0" });
        sep(&mut hash, 0x1f);
        feed(&mut hash, if entry.fit_excluded { "1" } else { "0" });
    }
    u64::from(hash)
}

fn valid_correlation_echo(value: &serde_json::Value) -> String {
    value
        .get("correlation_id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| CorrelationId::parse(id).is_some())
        .unwrap_or_default()
        .to_owned()
}

fn parse_direction(value: &str) -> Option<Direction> {
    match value {
        "left" => Some(Direction::Left),
        "right" => Some(Direction::Right),
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}

fn direction_str(value: Direction) -> &'static str {
    match value {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

/// Bounded client size-hint extent (AR12): `{w, h}` device units mirroring
/// the adapter `PlanWindowConstraints` min/max shapes. Unknown fields refuse
/// fail-closed; validation of meaningfulness lives in
/// [`tiler_core::size_hints`] (non-positive or absurd values behave as
/// absent, never as rejections: hints are advisory).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SizeDto {
    w: i32,
    h: i32,
}

fn parse_mode(value: &str) -> Option<tiler_core::contract::ResizeMode> {
    match value {
        "inwards" => Some(tiler_core::contract::ResizeMode::Inwards),
        "outwards" => Some(tiler_core::contract::ResizeMode::Outwards),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RectDto {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainDto {
    output: String,
    workspace: String,
    bounds: RectDto,
    gap: i32,
    #[serde(default)]
    outer_gap: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedDto {
    window: String,
    output: String,
    workspace: String,
    rect: RectDto,
    #[serde(default)]
    floating: bool,
    /// Native fullscreen overlay for the Q2 fixed-size classifier (D5):
    /// born fullscreen bypasses fixed admission. Defaults false so
    /// existing fixtures parse unchanged.
    #[serde(default)]
    fullscreen: bool,
    /// Native maximized overlay (R-WS-12 migration gate): carried so the
    /// whole-workspace refusal detects maximized floats independently of
    /// the `fit_excluded` proxy. Defaults false so existing fixtures parse
    /// unchanged.
    #[serde(default)]
    maximized: bool,
    /// Native sticky state for the Q2 fixed-size classifier (D3/D6):
    /// sticky floats are intentional, never automatic. Defaults false.
    #[serde(default)]
    sticky: bool,
    /// Adapter-asserted automatic fixed-float origin (Q2, D6): the
    /// adapter classified this window automatic. Core never infers
    /// automatic origin from hints alone. Defaults false so existing
    /// fixtures parse unchanged.
    #[serde(default)]
    fixed_auto: bool,
    /// Adapter-asserted user tile win (Q2, D3): the adapter retains an
    /// explicit tile override for this live client across hide, domain
    /// release, and workspace re-adoption. Defaults false.
    #[serde(default)]
    fixed_suppress: bool,
    /// Internal fit opt-out carried by the adapter for any floating, sticky,
    /// fullscreen, or maximized snapshot entry. Defaults false so existing
    /// fixtures parse unchanged; any set entry declines fitting while the
    /// normal seed/reflow exception behavior is untouched.
    #[serde(default)]
    fit_excluded: bool,
    /// AR12 client size hints. Both default absent so existing fixtures parse
    /// unchanged; unknown nested fields refuse via [`SizeDto`].
    /// Meaningfulness (positive, in-bound) is judged in
    /// [`tiler_core::size_hints`], never here. Hints never join the
    /// directional fingerprint (they are advisory, not layout identity), so
    /// existing adapter baselines keep matching.
    #[serde(default)]
    min_size: Option<SizeDto>,
    #[serde(default)]
    max_size: Option<SizeDto>,
}

/// Production directional domains payload (DescribePlan active route only).
/// Bounded primitive per domain: output, workspace, work-area bounds, inner
/// and outer gaps, plus reciprocal adjacency (`left`/`right`/`up`/`down` to an
/// output name). At most two domains: source first (must equal `domain`),
/// then the adjacent output's current logical workspace (which
/// may differ in workspace id). Four-direction adjacency (item 5.2) carries
/// FULL output rectangles resolved adapter-side; panel gaps never block.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectionalDomainDto {
    output: String,
    workspace: String,
    bounds: RectDto,
    gap: i32,
    #[serde(default)]
    outer_gap: i32,
    #[serde(default)]
    adjacent: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestDto {
    v: u32,
    correlation_id: String,
    owner: String,
    generation: String,
    revision: u64,
    fingerprint: u64,
    domain: DomainDto,
    #[serde(default)]
    target_domain: Option<DomainDto>,
    #[serde(default)]
    target_windows: Vec<ObservedDto>,
    /// Production directional cross-output observation: source plus at most
    /// one reciprocal adjacent domain. Absent for legacy
    /// single-domain requests, whose behavior is unchanged.
    #[serde(default)]
    domains: Option<Vec<DirectionalDomainDto>>,
    focused_window: String,
    windows: Vec<ObservedDto>,
    /// D23 close-versus-minimize distinction: opaque ids the adapter reports
    /// as explicitly closed (never merely absent). Absent (default empty)
    /// preserves the historical minimize-on-omission behavior; a closed id
    /// never mints a permanent slot and any stored slot for it is forgotten.
    #[serde(default)]
    closed_windows: Vec<String>,
    command: serde_json::Value,
    /// R-SPC-04 D1 fixed-size admission predicate (`both-axes-fixed` /
    /// `either-axis-fixed`). Absent preserves the historical both-axes
    /// behavior; unknown values refuse fail-closed before any state change.
    #[serde(default = "default_fixed_size_predicate")]
    fixed_size_predicate: String,
}

#[derive(Debug, Clone, Serialize)]
struct GeometryReply {
    window: String,
    leaf: String,
    output: String,
    workspace: String,
    rect: RectDto,
    /// AR12: the window's minimums are unsatisfiable in this allocation
    /// (proportional fallback shown). The adapter must never reassert this
    /// window. Absent unless true, so existing replies are byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    overconstrained: Option<bool>,
    /// AR12: the observed rectangle accepts as a client clamp of the desired
    /// rectangle (reconcile/update-gaps family only). The adapter must
    /// neither rewrite the window nor count it toward drift/park. Absent
    /// unless true, so existing replies are byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    client_clamped: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct FocusReplyBody {
    domain_output: String,
    domain_workspace: String,
    leaf: String,
}

#[derive(Debug, Clone, Serialize)]
struct FloatReplyBody {
    window: String,
    rect: RectDto,
}

#[derive(Debug, Clone, Serialize)]
struct PlanReply {
    v: u32,
    correlation_id: String,
    outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    desired_geometry: Option<Vec<GeometryReply>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    desired_focus: Option<FocusReplyBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    float_geometry: Option<FloatReplyBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preconditions: Option<Vec<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<serde_json::Value>,
    /// Read-only drag-preview proposed source rectangle (present only on the
    /// `preview` outcome; absent elsewhere so existing replies stay
    /// byte-identical).
    #[serde(skip_serializing_if = "Option::is_none")]
    preview_rect: Option<RectDto>,
    /// Read-only drag-preview carried hover state for the next preview or
    /// the final drop (present only on the `preview` outcome; shared JSON
    /// detail riding the existing reply).
    #[serde(skip_serializing_if = "Option::is_none")]
    hover_prior: Option<serde_json::Value>,
}

fn serialize_bounded(reply: &PlanReply) -> String {
    match serde_json::to_string(reply) {
        Ok(text) if text.len() <= PLAN_MAX_REPLY_BYTES => text,
        // Fixed bounded correlated rejection: a valid large request can
        // overflow the 64 KiB reply cap, and losing correlation there would
        // leave the transaction unattributable. Only a validated correlation
        // is ever echoed (garbage degrades to empty), so untrusted bytes
        // never escape through this path.
        _ => {
            let correlation = CorrelationId::parse(&reply.correlation_id)
                .map(|id| id.as_str().to_owned())
                .unwrap_or_default();
            serde_json::json!({
                "v": PLAN_CONTRACT_VERSION,
                "correlation_id": correlation,
                "outcome": "rejected",
                "kind": "reply-oversize",
                "message": "reply exceeds size bound",
            })
            .to_string()
        }
    }
}

/// Bounded redacted DescribePlan summary lines for the normal-level service
/// sink (Planner stderr, captured via the dev `planner-log` pointer into the
/// combined `[planner]` stream).
///
/// A complete request/reply
/// pair carries window ids, frame rectangles, domains, owner, and generation
/// in cleartext, which exceeds every other surface's redaction posture (KWin
/// logs opaque ids at most, never rects, owners, or payload bytes). The
/// summaries below are the only Planner-side record: op/outcome/kind tokens,
/// a validated correlation, bounded integers, and carried-entry counts.
/// Window/native ids, geometry, domains, owner, app/caption/secret data, and
/// raw payloads never appear. Both functions are pure and total over
/// arbitrary input: unparseable or out-of-shape sides degrade to bounded
/// placeholders, so malformed and unauthorized-shaped inputs cannot echo.
pub const PLAN_SUMMARY_PREFIX: &str = "omnitiler:plan-summary";

/// Sanitize one free-form token for summary lines: lowercase dashes only,
/// capped at 64 chars, else `unknown`. Mirrors the adapter-side rejection
/// token vocabulary. Never echoes payload bytes.
fn summary_token(raw: Option<&str>) -> String {
    match raw {
        Some(text)
            if !text.is_empty()
                && text.len() <= 64
                && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') =>
        {
            text.to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

/// Validated correlation for summary lines, else `-`. Only a well-formed
/// correlation id is ever emitted, so garbage (including unauthorized or
/// malformed requests) never echoes. This is the cross-service lookup key
/// shared with the KWin `correlation=` fields.
fn summary_correlation(raw: Option<&str>) -> String {
    match raw.and_then(CorrelationId::parse) {
        Some(id) => id.as_str().to_owned(),
        None => "-".to_owned(),
    }
}

/// Bounded revision for summary lines, else `-`.
fn summary_revision(value: Option<u64>) -> String {
    match value {
        Some(n) if n <= PLAN_MAX_REVISION => n.to_string(),
        _ => "-".to_owned(),
    }
}

/// Carried-entry count for summary lines. The request itself is size-bounded,
/// so the count is exact without enumerating anything sensitive.
fn summary_count(value: Option<&serde_json::Value>) -> String {
    match value.and_then(serde_json::Value::as_array) {
        Some(items) => items.len().to_string(),
        None => "0".to_owned(),
    }
}

fn summary_request_field<'a>(
    request: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    request.as_object().and_then(|object| object.get(key))
}

/// Normal-level ingress summary: requested op, validated correlation, bounded
/// revision. Emitted for every authorized `DescribePlan` call with truthful
/// op tokens.
#[must_use]
pub fn summarize_plan_ingress(request_json: &str) -> String {
    let request: serde_json::Value = serde_json::from_str(request_json).unwrap_or_default();
    format!(
        "{PLAN_SUMMARY_PREFIX} direction=ingress op={} correlation={} revision={}",
        summary_token(
            summary_request_field(&request, "command")
                .and_then(|command| command.get("op"))
                .and_then(serde_json::Value::as_str)
        ),
        summary_correlation(
            summary_request_field(&request, "correlation_id").and_then(serde_json::Value::as_str)
        ),
        summary_revision(
            summary_request_field(&request, "revision").and_then(serde_json::Value::as_u64)
        ),
    )
}

/// Opt-in structural ingress detail (trace wiring only): carried observation
/// shape as entry counts plus the numeric fingerprint. Counts and integers
/// only; never ids, rects, domains, owner, or payload bytes.
#[must_use]
pub fn summarize_plan_shape(request_json: &str) -> String {
    let request: serde_json::Value = serde_json::from_str(request_json).unwrap_or_default();
    format!(
        "{PLAN_SUMMARY_PREFIX} direction=shape op={} correlation={} windows={} target_windows={} domains={} fingerprint={}",
        summary_token(
            summary_request_field(&request, "command")
                .and_then(|command| command.get("op"))
                .and_then(serde_json::Value::as_str)
        ),
        summary_correlation(
            summary_request_field(&request, "correlation_id").and_then(serde_json::Value::as_str)
        ),
        summary_count(summary_request_field(&request, "windows")),
        summary_count(summary_request_field(&request, "target_windows")),
        summary_count(summary_request_field(&request, "domains")),
        match summary_request_field(&request, "fingerprint").and_then(serde_json::Value::as_u64) {
            Some(n) => n.to_string(),
            None => "-".to_owned(),
        },
    )
}

/// Normal-level terminal summary: op re-parsed from the request (so the pair
/// stays attributable even when the reply carries no context), plus
/// correlation/outcome/kind/base/detail from the reply echo.
#[must_use]
pub fn summarize_plan_egress(request_json: &str, reply_json: &str) -> String {
    let request: serde_json::Value = serde_json::from_str(request_json).unwrap_or_default();
    let reply: serde_json::Value = serde_json::from_str(reply_json).unwrap_or_default();
    let reply_field = |key: &str| reply.as_object().and_then(|object| object.get(key));
    format!(
        "{PLAN_SUMMARY_PREFIX} direction=egress op={} correlation={} outcome={} kind={} base_revision={} detail={}",
        summary_token(
            summary_request_field(&request, "command")
                .and_then(|command| command.get("op"))
                .and_then(serde_json::Value::as_str)
        ),
        summary_correlation(reply_field("correlation_id").and_then(serde_json::Value::as_str)),
        summary_token(reply_field("outcome").and_then(serde_json::Value::as_str)),
        match reply_field("kind").and_then(serde_json::Value::as_str) {
            Some(kind) => summary_token(Some(kind)),
            None => "-".to_owned(),
        },
        summary_revision(reply_field("base_revision").and_then(serde_json::Value::as_u64)),
        match reply_field("detail") {
            Some(serde_json::Value::String(detail)) => summary_token(Some(detail)),
            _ => "-".to_owned(),
        },
    )
}

/// Normal-level convergence summary for complete-observation convergence
/// (`docs/changes/archive/observation-convergence.md`): bounded correlated counts
/// emitted at the Planner protocol boundary after [`Engine::handle`]
/// converges retained membership/floating state to the complete current
/// observation. Counts only (removed/admitted/flag-adopted) plus the reason
/// op and the validated correlation; never native identifiers, geometry,
/// domains, owner, or raw payloads. Pure and total like the other summaries:
/// unparseable sides degrade to bounded placeholders. The Engine records a
/// report only for nonzero convergence, so exact-match operations emit
/// nothing and stay at the ingress/egress pair.
#[must_use]
pub fn summarize_plan_convergence(
    correlation: &str,
    op: &str,
    removed: usize,
    admitted: usize,
    flags_adopted: usize,
) -> String {
    format!(
        "{PLAN_SUMMARY_PREFIX} direction=convergence op={} correlation={} reason=observation-mismatch removed={} admitted={} flags_adopted={}",
        summary_token(Some(op)),
        summary_correlation(Some(correlation)),
        removed,
        admitted,
        flags_adopted,
    )
}

/// Emit the bounded correlated convergence summary for the just-completed
/// [`Engine::handle`] call, if it converged with nonzero counts. Called at
/// the Planner protocol boundary after every Engine handle return; exact
/// matches and non-converging routes record nothing so they stay silent. No
/// reply field is added: the summary is log-only.
fn emit_engine_convergence(engine: &Engine) {
    if let Some(report) = engine.last_convergence() {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "{}",
            summarize_plan_convergence(
                report.correlation.as_str(),
                report.op,
                report.removed,
                report.admitted,
                report.flags_adopted,
            )
        );
    }
}

/// Bounded fresh adoption-fit prefix (normal-level, log-only).
pub const ADOPTION_FIT_PREFIX: &str = "omnitiler:adoption-fit";

/// Sanitize one adoption-fit token: lowercase/digits with dashes or
/// underscores, capped at 64 chars, else `unknown`. Covers the `ok` and
/// `single_window`-style reason vocabulary. Never echoes payload bytes.
fn adoption_token(raw: Option<&str>) -> String {
    match raw {
        Some(text)
            if !text.is_empty()
                && text.len() <= 64
                && text.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                }) =>
        {
            text.to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

/// Normal-level fresh adoption-fit summary: exactly one per actual fresh
/// adoption attempt, owned by the Engine decision and emitted at
/// `handle_and_serialize` after [`Engine::handle`]. Correlated, with the
/// actual commit result (`fitted` only when the fit committed, else
/// `fallback`), the complete carried window count, and the decline reason.
/// No raw ids or payloads. Pure and total: malformed sides degrade to
/// bounded placeholders.
#[must_use]
pub fn summarize_adoption_fit(
    correlation: &str,
    outcome: &str,
    windows: usize,
    reason: &str,
    centre_splits: usize,
) -> String {
    format!(
        "{ADOPTION_FIT_PREFIX} outcome={} windows={} reason={} centre_splits={} correlation={}",
        adoption_token(Some(outcome)),
        windows,
        adoption_token(Some(reason)),
        centre_splits,
        summary_correlation(Some(correlation)),
    )
}

/// Bounded fixed-size admission prefix (normal-level, log-only, D1-D8).
pub const FIXED_ADMISSION_PREFIX: &str = "omnitiler:fixed-size-admission";

/// Normal-level fixed-size admission summary: exactly one per op that
/// admitted automatic fixed floats with the opt-in enabled, owned by the
/// Engine decision and emitted at `handle_and_serialize`. Correlated,
/// with the evaluated/admitted counts and the fixed decision token.
/// No raw ids, geometry, domains, or payloads. Pure and total:
/// malformed sides degrade to bounded placeholders. Only admission ops
/// record, so ordinary frames stay silent (no per-frame logs).
#[must_use]
pub fn summarize_fixed_admission(
    correlation: &str,
    op: &str,
    evaluated: usize,
    admitted: usize,
    reason: &str,
) -> String {
    format!(
        "{FIXED_ADMISSION_PREFIX} op={} correlation={} evaluated={} admitted={} reason={}",
        adoption_token(Some(op)),
        summary_correlation(Some(correlation)),
        evaluated,
        admitted,
        adoption_token(Some(reason)),
    )
}

/// Emit the bounded correlated fixed-size admission summary for the
/// just-completed [`Engine::handle`] call, if it admitted automatic
/// fixed floats. Log-only.
fn emit_engine_fixed_admission(engine: &Engine) {
    if let Some(report) = engine.last_fixed_admission() {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "{}",
            summarize_fixed_admission(
                report.correlation.as_str(),
                report.op,
                report.evaluated,
                report.admitted,
                report.reason,
            )
        );
    }
}

/// Normal-level whole-workspace migration summary (R-WS-12): exactly one per
/// successful explicit migration, owned by the Engine decision and emitted at
/// `handle_and_serialize`. It records the retained rekey only, never native
/// completion: the adapter transfers natively afterwards under
/// `adapter-must-verify-postconditions`. Correlated direction plus relocated
/// member/float counts and the empty-domain marker; never identifiers,
/// geometry, domains, owner, or payloads.
#[must_use]
pub fn summarize_plan_migration(
    correlation: &str,
    direction: &str,
    members: usize,
    floats: usize,
    empty: bool,
) -> String {
    format!(
        "{PLAN_SUMMARY_PREFIX} direction=migration op=migrate-workspace correlation={} migrate_direction={} members={} floats={} empty={}",
        summary_correlation(Some(correlation)),
        summary_token(Some(direction)),
        members,
        floats,
        if empty { "true" } else { "false" },
    )
}

/// Emit the bounded correlated migration summary for the just-completed
/// [`Engine::handle`] call, if it completed an explicit migration.
/// Log-only; refusals record nothing so they stay at the ingress/egress pair.
fn emit_engine_migration(engine: &Engine) {
    if let Some(report) = engine.last_migration() {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "{}",
            summarize_plan_migration(
                report.correlation.as_str(),
                report.direction,
                report.members,
                report.floats,
                report.empty,
            )
        );
    }
}

/// Read-only drag-preview stage diagnostic (log-only, no wire change):
/// one line per stage transition plus a counted flush of any suppressed
/// run. Closed stage token plus the suppressed span's first/last validated
/// correlations; never identifiers, geometry, domains, owner, or payloads.
#[must_use]
pub fn summarize_drag_preview_stage(
    correlation: &str,
    stage: &str,
    suppressed: u64,
    span_first: &str,
    span_last: &str,
) -> String {
    format!(
        "{PLAN_SUMMARY_PREFIX} direction=drag-stage op=drag-preview correlation={} stage={} suppressed={suppressed} span_first={} span_last={}",
        summary_correlation(Some(correlation)),
        adoption_token(Some(stage)),
        summary_correlation(Some(span_first)),
        summary_correlation(Some(span_last)),
    )
}

/// Transition-gated stage runs: consecutive identical stages coalesce;
/// a stage change, reset, or the periodic bound flushes the run with its
/// stored correlation span.
#[derive(Debug, Default)]
struct PreviewStageDedup {
    last: Option<&'static str>,
    suppressed: u64,
    span_first: Option<String>,
    span_last: Option<String>,
}

impl PreviewStageDedup {
    /// Maximum silent repeats before a periodic count flush keeps an
    /// unterminated burst observable.
    const FLUSH_BOUND: u64 = 64;

    /// Classify one refused-preview stage into the exact stderr lines to
    /// emit, in order. Pure over closed tokens; never echoes payloads.
    fn observe(&mut self, correlation: &str, stage: &'static str) -> Vec<String> {
        if self.last == Some(stage) {
            self.suppressed += 1;
            let validated = summary_correlation(Some(correlation));
            if self.span_first.is_none() {
                self.span_first = Some(validated.clone());
            }
            self.span_last = Some(validated);
            if self.suppressed >= Self::FLUSH_BOUND {
                return self.take_run(correlation);
            }
            return Vec::new();
        }
        let mut lines = self.take_run(correlation);
        self.last = Some(stage);
        self.suppressed = 0;
        self.span_first = None;
        self.span_last = None;
        lines.push(summarize_drag_preview_stage(
            correlation,
            stage,
            0,
            "-",
            "-",
        ));
        lines
    }

    /// Flush any pending run at a success/non-preview boundary. Pure.
    fn reset(&mut self, correlation: &str) -> Vec<String> {
        self.take_run(correlation)
    }

    /// Take the pending run as counted lines, if any. Clears the run. The
    /// flush keeps the run's stored span; `correlation` only names the
    /// boundary for pipeline continuity.
    fn take_run(&mut self, correlation: &str) -> Vec<String> {
        let Some(last) = self.last.take() else {
            return Vec::new();
        };
        let suppressed = self.suppressed;
        self.suppressed = 0;
        let first = self.span_first.take().unwrap_or_else(|| "-".to_owned());
        let last_span = self.span_last.take().unwrap_or_else(|| "-".to_owned());
        if suppressed == 0 {
            return Vec::new();
        }
        vec![summarize_drag_preview_stage(
            correlation,
            last,
            suppressed,
            &first,
            &last_span,
        )]
    }
}

/// Emit the bounded correlated adoption-fit summary for the just-completed
/// [`Engine::handle`] call, if it attempted a fresh adoption. Retained
/// reconciliations record nothing so they stay silent. Log-only.
fn emit_engine_adoption_fit(engine: &Engine) {
    if let Some(report) = engine.last_adoption_fit() {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "{}",
            summarize_adoption_fit(
                report.correlation.as_str(),
                report.outcome,
                report.windows,
                report.reason,
                report.centre_splits,
            )
        );
    }
}

/// Trace-only placement prefix (opt-in trace wiring only, never normal logs).
pub const PLACEMENT_TRACE_PREFIX: &str = "omnitiler:placement-trace";

/// Whether trace-only placement diagnostics are enabled (`1` only).
/// Mirrors the Planner service trace gate so the protocol choke point can
/// emit placement traces without new normal-level log volume.
#[must_use]
pub fn placement_trace_enabled() -> bool {
    matches!(
        std::env::var("OMNITILER_TRACE"),
        Ok(value) if value == "1"
    )
}

fn trace_rect(rect: &tiler_core::geometry::Rect) -> String {
    format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h)
}

/// Sanitized opaque token for trace lines: validated project token shape
/// only, else `unknown`. Covers window/anchor ids without echoing payloads.
fn trace_id(raw: &str) -> String {
    if tiler_core::bounds::is_opaque_id(raw) {
        raw.to_owned()
    } else {
        "unknown".to_owned()
    }
}

fn trace_axis(axis: tiler_core::directional::Axis) -> &'static str {
    match axis {
        tiler_core::directional::Axis::Horizontal => "horizontal",
        tiler_core::directional::Axis::Vertical => "vertical",
    }
}

/// Trace-only startup placement summary: bounded domain bounds, capped
/// carried inputs (opaque window token plus rectangle each), fit
/// outcome/reason, and the resulting ordered tree description with axes
/// and nested shares. Opaque tokens and integer geometry only. Pure.
#[must_use]
pub fn summarize_startup_fit_trace(report: &tiler_core::engine::EngineStartupFitTrace) -> String {
    let inputs = report
        .inputs
        .iter()
        .map(|input| format!("{}={}", trace_id(&input.window.0), trace_rect(&input.rect)))
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "{PLACEMENT_TRACE_PREFIX} kind=startup-fit correlation={} windows={} domain={} inputs={} outcome={} reason={} centre_splits={} leaves={} topology={}",
        summary_correlation(Some(report.correlation.as_str())),
        report.windows,
        trace_rect(&report.domain_bounds),
        if inputs.is_empty() {
            "-".to_owned()
        } else {
            inputs
        },
        adoption_token(Some(report.outcome)),
        adoption_token(Some(report.reason)),
        report.centre_splits,
        report.leaves,
        if report.topology.is_empty() {
            "-".to_owned()
        } else {
            report.topology.clone()
        },
    )
}

/// Trace-only send placement summary: selected anchor branch plus its
/// opaque leaf, admission axis, projected rectangle, and target leaf
/// count. Opaque tokens and integer geometry only. Pure.
#[must_use]
pub fn summarize_send_placement_trace(
    report: &tiler_core::engine::EngineSendPlacementTrace,
) -> String {
    format!(
        "{PLACEMENT_TRACE_PREFIX} kind=send-placement correlation={} anchor={} anchor_leaf={} axis={} projected={} target_leaves={}",
        summary_correlation(Some(report.correlation.as_str())),
        adoption_token(Some(report.anchor_kind)),
        report
            .anchor
            .as_ref()
            .map(|leaf| trace_id(&leaf.0))
            .unwrap_or_else(|| "-".to_owned()),
        trace_axis(report.axis),
        trace_rect(&report.projected),
        report.target_leaves,
    )
}

/// Trace-only placement lines for the just-completed [`Engine::handle`]
/// call: at most one startup-fit line plus at most one send-placement line.
/// Empty for retained ops with no fresh/send decision. Pure over the engine.
#[must_use]
pub fn placement_trace_lines(engine: &Engine) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(report) = engine.last_startup_fit_trace() {
        out.push(summarize_startup_fit_trace(report));
    }
    if let Some(report) = engine.last_send_placement() {
        out.push(summarize_send_placement_trace(report));
    }
    out
}

/// Emit trace-only placement diagnostics for the just-completed
/// [`Engine::handle`] call when the opt-in trace gate is set. Default off:
/// normal runs emit nothing here, keeping the journal surface at the
/// existing ingress/egress/convergence/adoption-fit lines.
fn emit_engine_placement_trace(engine: &Engine) {
    if !placement_trace_enabled() {
        return;
    }
    for line in placement_trace_lines(engine) {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{line}");
    }
}

fn rejected(correlation_id: String, kind: &str, message: &str) -> String {
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id,
        outcome: "rejected",
        kind: Some(kind.to_owned()),
        message: Some(message.to_owned()),
        base_revision: None,
        detail: None,
        desired_geometry: None,
        desired_focus: None,
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

fn snapshot_invalid(correlation_id: String, message: &str, detail: &'static str) -> String {
    debug_assert!(PLANNER_SNAPSHOT_DETAILS.contains(&detail));
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id,
        outcome: "rejected",
        kind: Some("snapshot-invalid".to_owned()),
        message: Some(message.to_owned()),
        base_revision: None,
        detail: Some(serde_json::Value::String(detail.to_owned())),
        desired_geometry: None,
        desired_focus: None,
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

const PLANNER_SNAPSHOT_DETAILS: &[&str] = &[
    "domain-output-invalid",
    "domain-workspace-invalid",
    "focused-id-invalid",
    "observed-window-invalid",
    "observed-output-invalid",
    "observed-workspace-invalid",
    "duplicate-window",
    "domain-bounds-invalid",
    "gap-low",
    "gap-high",
    "outer-gap-low",
    "outer-gap-high",
    "window-rect-invalid",
    "focused-not-observed",
    "fingerprint-mismatch",
    "inset-exhausted",
    "domain-invalid",
    "canonical-state-unavailable",
    "canonical-source-unavailable",
    "canonical-pair-domain-mismatch",
    "canonical-pair-identity-mismatch",
    "canonical-pair-unusable",
    "canonical-pair-duplicate-state",
    "commit-rejected",
    "missing-seed-order",
    "seed-failed",
    "placement-bounds-invalid",
    "admit-op-invalid",
    "admit-window-invalid",
    "admit-output-invalid",
    "admit-workspace-invalid",
    "remove-op-invalid",
    "remove-window-invalid",
    "move-op-invalid",
    "move-window-invalid",
    "move-output-invalid",
    "move-workspace-invalid",
    "focus-op-invalid",
    "focus-window-invalid",
    "resize-op-invalid",
    "resize-window-invalid",
    "pointer-resize-op-invalid",
    "pointer-resize-window-invalid",
    "reconcile-op-invalid",
    "active-group-op-invalid",
    "toggle-float-op-invalid",
    "toggle-float-window-invalid",
    "toggle-orient-op-invalid",
    "toggle-orient-window-invalid",
    "drag-drop-op-invalid",
    "drag-drop-window-invalid",
    "drag-preview-op-invalid",
    "float-rect-invalid",
    "fixed-predicate-invalid",
];

fn classify_parse_error(error: &serde_json::Error) -> (&'static str, &'static str) {
    let text = error.to_string();
    if text.contains("unknown field") {
        ("unknown-field", MSG_UNKNOWN_FIELD)
    } else if text.contains("unknown variant") {
        ("unknown-value", MSG_UNKNOWN_VALUE)
    } else {
        ("request-malformed", MSG_MALFORMED)
    }
}

fn geometry_reply(g: &tiler_core::session::DesiredGeometry) -> GeometryReply {
    GeometryReply {
        window: g.window.0.clone(),
        leaf: g.leaf.0.clone(),
        output: g.output.0.clone(),
        workspace: g.workspace.0.clone(),
        rect: RectDto {
            x: g.rect.x,
            y: g.rect.y,
            w: g.rect.w,
            h: g.rect.h,
        },
        overconstrained: g.overconstrained.then_some(true),
        client_clamped: g.client_clamped.then_some(true),
    }
}

fn focus_reply(domain: &DomainKey, leaf: &NodeId) -> FocusReplyBody {
    FocusReplyBody {
        domain_output: domain.output.0.clone(),
        domain_workspace: domain.workspace.0.clone(),
        leaf: leaf.0.clone(),
    }
}

/// Validated request for the retained evaluator.
struct Validated {
    request: RequestDto,
    raw: serde_json::Value,
    owner: OwnerId,
    generation: GenerationId,
    correlation: CorrelationId,
    domain: OutputDomain,
    domain_key: DomainKey,
    /// Production directional domains (source first) when the adapter sent a
    /// bounded `domains` payload on focus/move. `None` for legacy
    /// single-domain requests, whose behavior is unchanged.
    directional_domains: Option<Vec<OutputDomain>>,
    directional_keys: Option<Vec<DomainKey>>,
}

/// Parse one directional wire domain into its projected [`OutputDomain`].
/// Fails closed on any unreadable shape; all four cardinal adjacency keys
/// (`left`/`right`/`up`/`down`) are admitted for four-direction cross (item
/// 5.2). Anything else refuses.
fn parse_directional_domain(
    entry: &DirectionalDomainDto,
    correlation_id: &str,
) -> Result<(OutputDomain, DomainKey), String> {
    if !is_opaque_id(&entry.output) {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OPAQUE_ID,
            "domain-output-invalid",
        ));
    }
    if !is_opaque_id(&entry.workspace) {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OPAQUE_ID,
            "domain-workspace-invalid",
        ));
    }
    let carried = Rect {
        x: entry.bounds.x,
        y: entry.bounds.y,
        w: entry.bounds.w,
        h: entry.bounds.h,
    };
    if !valid_carried_rect(carried.x, carried.y, carried.w, carried.h) {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OBSERVATION,
            "domain-bounds-invalid",
        ));
    }
    if entry.gap < 0 {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OBSERVATION,
            "gap-low",
        ));
    }
    if entry.gap > GEOMETRY_MAX_GAP {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OBSERVATION,
            "gap-high",
        ));
    }
    if entry.outer_gap < 0 {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OBSERVATION,
            "outer-gap-low",
        ));
    }
    if entry.outer_gap > GEOMETRY_MAX_GAP {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OBSERVATION,
            "outer-gap-high",
        ));
    }
    let mut adjacent: std::collections::BTreeMap<Direction, OutputId> =
        std::collections::BTreeMap::new();
    for (key, target) in &entry.adjacent {
        let direction = match key.as_str() {
            "left" => Direction::Left,
            "right" => Direction::Right,
            "up" => Direction::Up,
            "down" => Direction::Down,
            _ => {
                return Err(snapshot_invalid(
                    correlation_id.to_owned(),
                    MSG_OBSERVATION,
                    "domain-invalid",
                ));
            }
        };
        if !is_opaque_id(target) {
            return Err(snapshot_invalid(
                correlation_id.to_owned(),
                MSG_OPAQUE_ID,
                "domain-output-invalid",
            ));
        }
        if adjacent
            .insert(direction, OutputId(target.clone()))
            .is_some()
        {
            return Err(snapshot_invalid(
                correlation_id.to_owned(),
                MSG_OBSERVATION,
                "domain-invalid",
            ));
        }
    }
    let Ok(projected) = tiler_core::geometry::inset_bounds(carried, entry.outer_gap) else {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OBSERVATION,
            "inset-exhausted",
        ));
    };
    let domain = OutputDomain {
        id: OutputId(entry.output.clone()),
        workspace: WorkspaceId(entry.workspace.clone()),
        bounds: projected,
        gap: entry.gap,
        adjacent,
    };
    if !domain.validate() {
        return Err(snapshot_invalid(
            correlation_id.to_owned(),
            MSG_OBSERVATION,
            "domain-invalid",
        ));
    }
    let key = DomainKey {
        output: OutputId(entry.output.clone()),
        workspace: WorkspaceId(entry.workspace.clone()),
    };
    Ok((domain, key))
}

/// Shared request validation: bounds, opaque ids, and domain binding.
/// Observed frame rectangles are host drift: valid carried rects are
/// accepted and converge to canonical planned geometry downstream.
/// Returns the ready-made rejected reply on failure.
fn validate_request(request_json: &str) -> Result<Validated, String> {
    if request_json.len() > PLAN_MAX_REQUEST_BYTES {
        return Err(rejected(String::new(), "oversized", MSG_OVERSIZED));
    }
    let raw: serde_json::Value = match serde_json::from_str(request_json) {
        Ok(raw) => raw,
        Err(error) => {
            let (kind, message) = classify_parse_error(&error);
            return Err(rejected(String::new(), kind, message));
        }
    };
    let request: RequestDto = match serde_json::from_value(raw.clone()) {
        Ok(request) => request,
        Err(error) => {
            let (kind, message) = classify_parse_error(&error);
            return Err(rejected(valid_correlation_echo(&raw), kind, message));
        }
    };
    if request.v != PLAN_CONTRACT_VERSION {
        return Err(rejected(
            request.correlation_id.clone(),
            "unsupported-version",
            MSG_VERSION,
        ));
    }
    if CorrelationId::parse(&request.correlation_id).is_none() {
        return Err(rejected(
            String::new(),
            "correlation-invalid",
            MSG_CORRELATION,
        ));
    }
    if OwnerId::parse(&request.owner).is_none() {
        return Err(rejected(
            request.correlation_id.clone(),
            "owner-invalid",
            MSG_OWNER,
        ));
    }
    if GenerationId::parse(&request.generation).is_none() {
        return Err(rejected(
            request.correlation_id.clone(),
            "generation-invalid",
            MSG_GENERATION,
        ));
    }
    if request.revision > PLAN_MAX_REVISION {
        return Err(rejected(
            request.correlation_id.clone(),
            "revision-invalid",
            MSG_REVISION,
        ));
    }
    if !is_opaque_id(&request.domain.output) {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OPAQUE_ID,
            "domain-output-invalid",
        ));
    }
    if !is_opaque_id(&request.domain.workspace) {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OPAQUE_ID,
            "domain-workspace-invalid",
        ));
    }
    if !request.focused_window.is_empty() && !is_opaque_id(&request.focused_window) {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OPAQUE_ID,
            "focused-id-invalid",
        ));
    }
    {
        let mut seen = std::collections::HashSet::new();
        for entry in &request.windows {
            if !is_opaque_id(&entry.window) {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OPAQUE_ID,
                    "observed-window-invalid",
                ));
            }
            if !is_opaque_id(&entry.output) {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OPAQUE_ID,
                    "observed-output-invalid",
                ));
            }
            if !is_opaque_id(&entry.workspace) {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OPAQUE_ID,
                    "observed-workspace-invalid",
                ));
            }
            if !seen.insert(entry.window.clone()) {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OPAQUE_ID,
                    "duplicate-window",
                ));
            }
        }
    }
    {
        // D23 closed-window validation, same fences as live observations:
        // opaque ids, no duplicates, and no contradictory live+closed row
        // (one id cannot be both observed and reported closed). Count rides
        // the request size bound like live windows; no new vocabulary.
        let mut seen = std::collections::HashSet::new();
        let live: std::collections::HashSet<&str> = request
            .windows
            .iter()
            .map(|entry| entry.window.as_str())
            .collect();
        for id in &request.closed_windows {
            if !is_opaque_id(id) {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OPAQUE_ID,
                    "observed-window-invalid",
                ));
            }
            if !seen.insert(id.clone()) || live.contains(id.as_str()) {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OPAQUE_ID,
                    "duplicate-window",
                ));
            }
        }
    }
    let carried_bounds = Rect {
        x: request.domain.bounds.x,
        y: request.domain.bounds.y,
        w: request.domain.bounds.w,
        h: request.domain.bounds.h,
    };
    if !valid_carried_rect(
        carried_bounds.x,
        carried_bounds.y,
        carried_bounds.w,
        carried_bounds.h,
    ) {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "domain-bounds-invalid",
        ));
    }
    if request.domain.gap < 0 {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "gap-low",
        ));
    }
    if request.domain.gap > GEOMETRY_MAX_GAP {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "gap-high",
        ));
    }
    if request.domain.outer_gap < 0 {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "outer-gap-low",
        ));
    }
    if request.domain.outer_gap > GEOMETRY_MAX_GAP {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "outer-gap-high",
        ));
    }
    let op_str = request
        .command
        .get("op")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    // Production directional payload: only focus/move may carry `domains`;
    // every other op (including the standalone workspace-send route) keeps
    // legacy single-domain behavior and refuses it fail-closed.
    let directional = match (&request.domains, op_str) {
        (None, _) => None,
        (Some(_), "focus") | (Some(_), "move") => request.domains.clone(),
        (Some(_), _) => {
            return Err(snapshot_invalid(
                request.correlation_id.clone(),
                MSG_OBSERVATION,
                "domain-invalid",
            ));
        }
    };
    // Parsed directional domains (source first), when present.
    let mut directional_parsed: Option<Vec<(OutputDomain, DomainKey)>> = None;
    if let Some(entries) = &directional {
        if entries.is_empty() || entries.len() > 2 {
            return Err(snapshot_invalid(
                request.correlation_id.clone(),
                MSG_OBSERVATION,
                "domain-invalid",
            ));
        }
        let mut parsed: Vec<(OutputDomain, DomainKey)> = Vec::with_capacity(entries.len());
        for entry in entries {
            parsed.push(parse_directional_domain(entry, &request.correlation_id)?);
        }
        // Source entry must equal the carried source domain (output,
        // workspace, raw bounds, gaps). Adjacency lives only in `domains`;
        // the inset-projected bounds are compared via the wire entry so the
        // raw work area must match exactly.
        {
            let wire_source = &entries[0];
            if wire_source.output != request.domain.output
                || wire_source.workspace != request.domain.workspace
                || wire_source.bounds.x != request.domain.bounds.x
                || wire_source.bounds.y != request.domain.bounds.y
                || wire_source.bounds.w != request.domain.bounds.w
                || wire_source.bounds.h != request.domain.bounds.h
                || wire_source.gap != request.domain.gap
                || wire_source.outer_gap != request.domain.outer_gap
            {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OBSERVATION,
                    "domain-invalid",
                ));
            }
        }
        // Distinct (output, workspace) pairs and distinct outputs.
        {
            let mut seen_pairs = std::collections::HashSet::new();
            let mut seen_outputs = std::collections::HashSet::new();
            for (domain, _) in &parsed {
                if !seen_pairs.insert((domain.id.0.clone(), domain.workspace.0.clone())) {
                    return Err(snapshot_invalid(
                        request.correlation_id.clone(),
                        MSG_OBSERVATION,
                        "domain-invalid",
                    ));
                }
                if !seen_outputs.insert(domain.id.0.clone()) {
                    // Ambiguous duplicate output ids fail closed.
                    return Err(snapshot_invalid(
                        request.correlation_id.clone(),
                        MSG_OBSERVATION,
                        "domain-invalid",
                    ));
                }
            }
        }
        // Two-domain reciprocity: source names target on one cardinal side
        // and the target names source back on the opposite side. Position-
        // based FULL-rectangle selection over multiple candidates is
        // adapter-owned (2026-10-09: window-centre, then span overlap, then
        // left/top; migration largest edge); the core only admits the
        // resolved reciprocal pair and refuses non-reciprocal topology
        // fail-closed. Valid reverse ambiguity (target touched by two
        // sources) stays accepted. Single-domain payloads carry no adjacency
        // requirement.
        if parsed.len() == 2 {
            let (source_domain, _) = &parsed[0];
            let (target_domain, _) = &parsed[1];
            let mut reciprocal = false;
            for direction in [
                Direction::Left,
                Direction::Right,
                Direction::Up,
                Direction::Down,
            ] {
                let opposite = match direction {
                    Direction::Left => Direction::Right,
                    Direction::Right => Direction::Left,
                    Direction::Up => Direction::Down,
                    Direction::Down => Direction::Up,
                };
                if source_domain.adjacent.get(&direction) == Some(&target_domain.id)
                    && target_domain.adjacent.get(&opposite) == Some(&source_domain.id)
                {
                    reciprocal = true;
                    break;
                }
            }
            if !reciprocal {
                return Err(snapshot_invalid(
                    request.correlation_id.clone(),
                    MSG_OBSERVATION,
                    "domain-invalid",
                ));
            }
        }
        directional_parsed = Some(parsed);
    }
    for entry in &request.windows {
        if !valid_carried_rect(entry.rect.x, entry.rect.y, entry.rect.w, entry.rect.h) {
            return Err(snapshot_invalid(
                request.correlation_id.clone(),
                MSG_OBSERVATION,
                "window-rect-invalid",
            ));
        }
        if let Some(parsed) = &directional_parsed {
            let mut homed = false;
            for (domain, _) in parsed.iter() {
                if entry.output == domain.id.0 && entry.workspace == domain.workspace.0 {
                    homed = true;
                    break;
                }
            }
            if !homed {
                return Err(rejected(
                    request.correlation_id.clone(),
                    "cross-domain-mismatch",
                    MSG_CROSS_DOMAIN,
                ));
            }
        } else {
            if entry.output != request.domain.output || entry.workspace != request.domain.workspace
            {
                return Err(rejected(
                    request.correlation_id.clone(),
                    "cross-domain-mismatch",
                    MSG_CROSS_DOMAIN,
                ));
            }
        }
    }
    // Whole-workspace migration permits an empty active client with
    // members (no fabricated focus); the Engine owns that gate.
    let migrate_no_active = op_str == "migrate-workspace" && request.focused_window.is_empty();
    if !migrate_no_active
        && !request.windows.is_empty()
        && !request
            .windows
            .iter()
            .any(|w| w.window == request.focused_window)
    {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "focused-not-observed",
        ));
    }
    // Directional focus/move: the focused window must live in the source
    // domain (cross targets are never the planning input). Binds the request
    // to the source so a stale target substitution cannot redirect planning.
    if directional_parsed.is_some()
        && !request.windows.iter().any(|w| {
            w.window == request.focused_window
                && w.output == request.domain.output
                && w.workspace == request.domain.workspace
        })
    {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "focused-not-observed",
        ));
    }
    // Directional fingerprint binding: recompute over the full two-domain
    // evidence (ordered domain primitives, focused id, every window) and
    // refuse altered target rects/bounds/adjacency here at request
    // validation, not only at reply revalidation downstream.
    if let Some(entries) = &directional {
        let expected = directional_fingerprint(entries, &request.focused_window, &request.windows);
        if request.fingerprint != expected {
            return Err(snapshot_invalid(
                request.correlation_id.clone(),
                MSG_OBSERVATION,
                "fingerprint-mismatch",
            ));
        }
    }
    let owner = OwnerId::parse(&request.owner).expect("validated");
    let generation = GenerationId::parse(&request.generation).expect("validated");
    let correlation = CorrelationId::parse(&request.correlation_id).expect("validated");
    // Rust owns the outer inset: an exhausted inset fails closed here while
    // segment overflow still fails in projection.
    let Ok(projected_bounds) =
        tiler_core::geometry::inset_bounds(carried_bounds, request.domain.outer_gap)
    else {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "inset-exhausted",
        ));
    };
    let domain = OutputDomain {
        id: OutputId(request.domain.output.clone()),
        workspace: WorkspaceId(request.domain.workspace.clone()),
        bounds: projected_bounds,
        gap: request.domain.gap,
        adjacent: std::collections::BTreeMap::new(),
    };
    if !domain.validate() {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_OBSERVATION,
            "domain-invalid",
        ));
    }
    let domain_key = DomainKey {
        output: OutputId(request.domain.output.clone()),
        workspace: WorkspaceId(request.domain.workspace.clone()),
    };
    // Directional domains/keys for the focus/move cross route. Single entry
    // means source-only (behaves like legacy); two entries carry the
    // reciprocal adjacent target.
    let (directional_domains, directional_keys) = match directional_parsed {
        Some(parsed) => {
            let mut domains = Vec::with_capacity(parsed.len());
            let mut keys = Vec::with_capacity(parsed.len());
            for (d, k) in parsed {
                domains.push(d);
                keys.push(k);
            }
            (Some(domains), Some(keys))
        }
        None => (None, None),
    };
    // R-SPC-04 D1 predicate: omitted carries the both-axes default via the
    // serde default above; unknown values refuse fail-closed before any
    // state change under one static detail.
    if tiler_core::size_hints::FixedSizePredicate::parse_wire(request.fixed_size_predicate.as_str())
        .is_none()
    {
        return Err(snapshot_invalid(
            request.correlation_id.clone(),
            MSG_UNKNOWN_VALUE,
            "fixed-predicate-invalid",
        ));
    }
    Ok(Validated {
        request,
        raw,
        owner,
        generation,
        correlation,
        domain,
        domain_key,
        directional_domains,
        directional_keys,
    })
}

fn validated_op(ctx: &Validated) -> String {
    ctx.request
        .command
        .get("op")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Wire token for a directional move precondition (production cross-output
/// route). Matches the portable movement-service vocabulary exactly.
fn move_precondition_str(value: tiler_core::directional::Precondition) -> &'static str {
    match value {
        tiler_core::directional::Precondition::FocusedLeafOccupiedByFocusedWindow => {
            "focused-leaf-occupied-by-focused-window"
        }
        tiler_core::directional::Precondition::NeighborLeafOccupied => "neighbor-leaf-occupied",
        tiler_core::directional::Precondition::ContainerIsDirectParent => "container-is-direct-parent",
        tiler_core::directional::Precondition::TargetGroupMembership => "target-group-membership",
        tiler_core::directional::Precondition::ParentGroupMembership => "parent-group-membership",
        tiler_core::directional::Precondition::SourceRootMembershipAndAdjacentSameWorkspaceOutput => {
            "source-root-membership-and-adjacent-same-workspace-output"
        }
        tiler_core::directional::Precondition::AdapterMustVerifyPostconditions => {
            "adapter-must-verify-postconditions"
        }
    }
}

/// Wire token for a focus precondition (production directional route).
fn focus_precondition_str(value: tiler_core::contract::FocusPrecondition) -> &'static str {
    match value {
        tiler_core::contract::FocusPrecondition::FocusedLeafOccupiedByFocusedWindow => {
            "focused-leaf-occupied-by-focused-window"
        }
        tiler_core::contract::FocusPrecondition::FocusedFloatingWindow => "focused-floating-window",
        tiler_core::contract::FocusPrecondition::TargetLeafOccupied => "target-leaf-occupied",
        tiler_core::contract::FocusPrecondition::FocusTargetsSameDomain => {
            "focus-targets-same-domain"
        }
        tiler_core::contract::FocusPrecondition::FocusTargetsAdjacentOutput => {
            "focus-targets-adjacent-output"
        }
        tiler_core::contract::FocusPrecondition::AdapterMustVerifyPostconditions => {
            "adapter-must-verify-postconditions"
        }
    }
}

/// Planned cross-output focus reply: full source+target geometry, target
/// focus, plus the exact operation/preconditions the adapter must fence
/// (target domain plus explicit cross source must match the captured source).
fn cross_focus_planned_reply(
    correlation_id: &str,
    base_revision: u64,
    detail: serde_json::Value,
    geometry: &[tiler_core::session::DesiredGeometry],
    focus: (&DomainKey, &NodeId),
    operation: &tiler_core::contract::FocusOperation,
) -> String {
    let operation_value = serde_json::json!({
        "op": "focus",
        "domain_output": operation.domain_output.0,
        "domain_workspace": operation.domain_workspace.0,
        "from_leaf": operation.from_leaf.as_ref().map(|id| id.0.clone()),
        "to_leaf": operation.to_leaf.0,
        "from_window": operation.from_window.0,
        "to_window": operation.to_window.0,
        "direction": direction_str(operation.direction),
        "route": operation.route.iter().map(|id| id.0.clone()).collect::<Vec<_>>(),
        "cross_source_output": operation.cross_source_output.as_ref().map(|id| id.0.clone()),
        "cross_source_workspace": operation.cross_source_workspace.as_ref().map(|id| id.0.clone()),
    });
    let preconditions: Vec<&'static str> = operation
        .preconditions()
        .iter()
        .map(|p| focus_precondition_str(*p))
        .collect();
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "planned",
        kind: None,
        message: None,
        base_revision: Some(base_revision),
        detail: Some(detail),
        desired_geometry: Some(geometry.iter().map(geometry_reply).collect()),
        desired_focus: Some(focus_reply(focus.0, focus.1)),
        float_geometry: None,
        preconditions: Some(preconditions),
        operation: Some(operation_value),
        preview_rect: None,
        hover_prior: None,
    })
}

/// Directional pair from a validated request: source + adjacent target when
/// the adapter sent two domains. Single-domain payloads return `None` and
/// keep legacy behavior.
fn directional_pair(
    ctx: &Validated,
) -> Option<(&OutputDomain, &DomainKey, &OutputDomain, &DomainKey)> {
    let domains = ctx.directional_domains.as_ref()?;
    let keys = ctx.directional_keys.as_ref()?;
    if domains.len() == 2 && keys.len() == 2 {
        Some((&domains[0], &keys[0], &domains[1], &keys[1]))
    } else {
        None
    }
}

fn planned_reply(
    correlation_id: &str,
    base_revision: u64,
    detail: serde_json::Value,
    geometry: &[tiler_core::session::DesiredGeometry],
    focus: Option<(&DomainKey, &NodeId)>,
) -> String {
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "planned",
        kind: None,
        message: None,
        base_revision: Some(base_revision),
        detail: Some(detail),
        desired_geometry: Some(geometry.iter().map(geometry_reply).collect()),
        desired_focus: focus.map(|(domain, leaf)| focus_reply(domain, leaf)),
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

/// Typed pure-projection serializer for the reconcile/update-gaps family:
/// funnels a [`tiler_core::boundary::ProjectionPlan`] through the exact
/// [`planned_reply`] wire shape, so output stays byte identical. Detail
/// `kind`/`capability` tokens come from the core plan (single source).
fn planned_projection_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::ProjectionPlan,
) -> String {
    planned_reply(
        correlation_id,
        plan.base_revision,
        serde_json::json!({
            "kind": plan.kind.kind_str(),
            "capability": plan.kind.capability_str(),
        }),
        &plan.geometry,
        match (&plan.focus_domain, &plan.focus_leaf) {
            (Some(domain), Some(leaf)) => Some((domain, leaf)),
            _ => None,
        },
    )
}

/// Typed versioned-tiled serializer for the toggle-float family:
/// funnels a [`tiler_core::boundary::TiledPlan`] through the exact planned
/// wire shape, so output stays byte identical. Detail
/// `kind`/`policy_version`/`capability` tokens come from the core plan
/// (single source); only kinds with fixed literal capabilities route here.
fn planned_tiled_reply(correlation_id: &str, plan: &tiler_core::boundary::TiledPlan) -> String {
    if plan.float_window.is_some() || plan.float_rect.is_some() {
        return planned_float_reply(correlation_id, plan);
    }
    planned_reply(
        correlation_id,
        plan.base_revision,
        serde_json::json!({
            "kind": plan.kind.kind_str(),
            "policy_version": plan.policy_version,
            "capability": plan
                .kind
                .capability_str()
                .expect("tiled reply kinds carry literal capabilities"),
        }),
        &plan.geometry,
        match (&plan.focus_domain, &plan.focus_leaf) {
            (Some(domain), Some(leaf)) => Some((domain, leaf)),
            _ => None,
        },
    )
}

/// Byte-exact intentional-float serializer: the legacy float wire shape
/// driven by a [`tiler_core::boundary::TiledPlan`] (single source).
fn planned_float_reply(correlation_id: &str, plan: &tiler_core::boundary::TiledPlan) -> String {
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "planned",
        kind: None,
        message: None,
        base_revision: Some(plan.base_revision),
        detail: Some(serde_json::json!({
            "kind": plan.kind.kind_str(),
            "policy_version": plan.policy_version,
            "capability": plan
                .kind
                .capability_str()
                .expect("tiled reply kinds carry literal capabilities"),
        })),
        desired_geometry: Some(plan.geometry.iter().map(geometry_reply).collect()),
        desired_focus: match (&plan.focus_domain, &plan.focus_leaf) {
            (Some(domain), Some(leaf)) => Some(focus_reply(domain, leaf)),
            _ => None,
        },
        float_geometry: match (&plan.float_window, &plan.float_rect) {
            (Some(window), Some(rect)) => Some(FloatReplyBody {
                window: window.0.clone(),
                rect: RectDto {
                    x: rect.x,
                    y: rect.y,
                    w: rect.w,
                    h: rect.h,
                },
            }),
            _ => None,
        },
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

/// Byte-exact local/cross move serializer driven by a
/// [`tiler_core::boundary::MovePlanReply`] (single source). Detail key order
/// (`kind`, `rule`, `capability`, `direction`) and the R4 cross operation
/// echo match the legacy shapes exactly; rule/capability `Debug` tokens are
/// formatted here in protocol from the same typed values.
fn serialize_move_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::MovePlanReply,
) -> String {
    let detail = serde_json::json!({
        "kind": "move",
        "rule": format!("{:?}", plan.rule),
        "capability": format!("{:?}", plan.capability),
        "direction": direction_str(plan.direction),
    });
    let focus = Some((&plan.focus_domain, &plan.focus_leaf));
    let Some(cross) = &plan.cross else {
        return planned_reply(
            correlation_id,
            plan.base_revision,
            detail,
            &plan.geometry,
            focus,
        );
    };
    let operation_value = serde_json::json!({
        "op": "move",
        "rule": format!("{:?}", cross.rule),
        "capability": format!("{:?}", plan.capability),
        "direction": direction_str(cross.intent_direction),
        "window": cross.intent_window.0,
        "leaf": cross.intent_leaf.0,
        "source_output": cross.source_output.0,
        "source_workspace": cross.source_workspace.0,
        "target_output": cross.target_output.0,
        "target_workspace": cross.target_workspace.0,
        "source_root_child_index": cross.source_root_child_index,
        "target": match cross.target {
            tiler_core::directional::CrossOutputTarget::Empty => "empty",
            tiler_core::directional::CrossOutputTarget::Occupied => "occupied",
        },
    });
    // The legacy adapter-must-verify-postconditions token binds the planned
    // operation shape. Immediate R4 commits do not await native verification;
    // KWin fences setters and follows only on fresh observed mover arrival.
    let preconditions: Vec<&'static str> = cross
        .preconditions
        .iter()
        .map(|p| move_precondition_str(*p))
        .collect();
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "planned",
        kind: None,
        message: None,
        base_revision: Some(plan.base_revision),
        detail: Some(detail),
        desired_geometry: Some(plan.geometry.iter().map(geometry_reply).collect()),
        desired_focus: focus.map(|(domain, leaf)| focus_reply(domain, leaf)),
        float_geometry: None,
        preconditions: Some(preconditions),
        operation: Some(operation_value),
        preview_rect: None,
        hover_prior: None,
    })
}

/// Byte-exact local/cross focus serializer driven by a
/// [`tiler_core::boundary::FocusPlanReply`] (single source). Detail key order
/// (`kind`, `capability`, `direction`, `to_window`, plus `cross_output` for
/// crossed plans) and the cross operation echo match the legacy shapes
/// exactly; precondition tokens derive from the carried operation.
fn serialize_focus_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::FocusPlanReply,
) -> String {
    let focus = Some((&plan.focus_domain, &plan.focus_leaf));
    let Some(operation) = &plan.cross_operation else {
        return planned_reply(
            correlation_id,
            plan.base_revision,
            serde_json::json!({
                "kind": "focus",
                "capability": "directional-focus",
                "direction": direction_str(plan.direction),
                "to_window": plan.to_window.0,
            }),
            &plan.geometry,
            focus,
        );
    };
    cross_focus_planned_reply(
        correlation_id,
        plan.base_revision,
        serde_json::json!({
            "kind": "focus",
            "capability": "directional-focus",
            "direction": direction_str(plan.direction),
            "to_window": plan.to_window.0,
            "cross_output": true,
        }),
        &plan.geometry,
        (&plan.focus_domain, &plan.focus_leaf),
        operation,
    )
}

/// Byte-exact keyboard/pointer resize serializer driven by a
/// [`tiler_core::boundary::ResizePlanReply`] (single source). Detail key
/// order matches the legacy shapes exactly; exactly one of mode/boundary is
/// set by construction.
fn serialize_resize_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::ResizePlanReply,
) -> String {
    let focus = Some((&plan.focus_domain, &plan.focus_leaf));
    let operation = &plan.operation;
    if let Some(mode) = plan.mode {
        return planned_reply(
            correlation_id,
            plan.base_revision,
            serde_json::json!({
                "kind": "resize",
                "capability": "keyboard-resize",
                "direction": direction_str(plan.direction),
                "mode": mode.as_str(),
                "target_group": operation.target_group.0,
                "focused_index": operation.focused_index,
                "neighbor_index": operation.neighbor_index,
                "old_shares": operation.old_shares,
                "new_shares": operation.new_shares,
            }),
            &plan.geometry,
            focus,
        );
    }
    debug_assert!(plan.boundary.is_some(), "pointer plans carry a boundary");
    if let Some(secondary) = &plan.secondary {
        return planned_reply(
            correlation_id,
            plan.base_revision,
            serde_json::json!({
                "kind": "pointer-resize",
                "capability": "pointer-resize",
                "direction": direction_str(plan.direction),
                "boundary": plan.boundary.unwrap_or(0),
                "target_group": operation.target_group.0,
                "focused_index": operation.focused_index,
                "neighbor_index": operation.neighbor_index,
                "old_shares": operation.old_shares,
                "new_shares": operation.new_shares,
                "direction2": direction_str(secondary.direction),
                "boundary2": secondary.boundary,
                "target_group2": secondary.operation.target_group.0,
                "focused_index2": secondary.operation.focused_index,
                "neighbor_index2": secondary.operation.neighbor_index,
                "old_shares2": secondary.operation.old_shares,
                "new_shares2": secondary.operation.new_shares,
            }),
            &plan.geometry,
            focus,
        );
    }
    planned_reply(
        correlation_id,
        plan.base_revision,
        serde_json::json!({
            "kind": "pointer-resize",
            "capability": "pointer-resize",
            "direction": direction_str(plan.direction),
            "boundary": plan.boundary.unwrap_or(0),
            "target_group": operation.target_group.0,
            "focused_index": operation.focused_index,
            "neighbor_index": operation.neighbor_index,
            "old_shares": operation.old_shares,
            "new_shares": operation.new_shares,
        }),
        &plan.geometry,
        focus,
    )
}

/// Workspace-send serializer driven by a
/// [`tiler_core::boundary::SendWorkspacePlan`] (single source). Detail key
/// order (`kind`, `policy_version`, `capability`), the `move-tiled` operation
/// echo (plus the explicit `follow` selection), and precondition tokens match
/// the legacy shape plus the follow extension. The slotless `move-deferred`
/// transfer (D28 born-fullscreen carry) echoes window plus source/target
/// identities with NO leaf under its own op token; capability stays
/// `move-tiled` (the operation reuses that capability). The plan
/// constructor guarantees one of the two send operations, so the legacy
/// `move-op-invalid` fallback stays with the caller at its exact position.
fn serialize_send_workspace_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::SendWorkspacePlan,
) -> String {
    let operation_value = match &plan.operation {
        LifecycleOperation::MoveTiled {
            window,
            leaf,
            source_output,
            source_workspace,
            target_output,
            target_workspace,
        } => serde_json::json!({
            "op": "move-tiled",
            "window": window.0,
            "leaf": leaf.0,
            "source_output": source_output.0,
            "source_workspace": source_workspace.0,
            "target_output": target_output.0,
            "target_workspace": target_workspace.0,
            "follow": plan.follow,
        }),
        LifecycleOperation::MoveDeferred {
            window,
            source_output,
            source_workspace,
            target_output,
            target_workspace,
        } => serde_json::json!({
            "op": "move-deferred",
            "window": window.0,
            "source_output": source_output.0,
            "source_workspace": source_workspace.0,
            "target_output": target_output.0,
            "target_workspace": target_workspace.0,
            "follow": plan.follow,
        }),
        _ => unreachable!("SendWorkspacePlan always carries a send move operation"),
    };
    // Keep the legacy wire token for exact reply binding, not as a native
    // verification or committed-native-success assertion. KWin reconciles
    // both domains after the immediate planned-topology commit.
    let preconditions: Vec<&'static str> = plan
        .preconditions
        .iter()
        .map(|p| lifecycle_precondition_str(*p))
        .collect();
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "planned",
        kind: Some("send-to-workspace".to_owned()),
        message: None,
        base_revision: Some(plan.base_revision),
        detail: Some(serde_json::json!({
            "kind": "send-to-workspace",
            "policy_version": plan.policy_version,
            "capability": "move-tiled",
        })),
        desired_geometry: Some(plan.geometry.iter().map(geometry_reply).collect()),
        desired_focus: match (&plan.focus_domain, &plan.focus_leaf) {
            (Some(d), Some(l)) => Some(focus_reply(d, l)),
            _ => None,
        },
        float_geometry: None,
        preconditions: Some(preconditions),
        operation: Some(operation_value),
        preview_rect: None,
        hover_prior: None,
    })
}

/// Explicit output-send serializer driven by a
/// [`tiler_core::boundary::SendWorkspacePlan`] (single source). Same ordinary
/// `move-tiled` transfer shape as [`serialize_send_workspace_reply`] (plus the
/// explicit `follow` selection) under the distinct `send-to-output` wire kind:
/// the destination output's current workspace resolved adapter-side. Detail
/// key order (`kind`, `policy_version`, `capability`) matches workspace send.
#[allow(clippy::too_many_lines)]
fn serialize_send_output_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::SendWorkspacePlan,
) -> String {
    let operation_value = match &plan.operation {
        LifecycleOperation::MoveTiled {
            window,
            leaf,
            source_output,
            source_workspace,
            target_output,
            target_workspace,
        } => serde_json::json!({
            "op": "move-tiled",
            "window": window.0,
            "leaf": leaf.0,
            "source_output": source_output.0,
            "source_workspace": source_workspace.0,
            "target_output": target_output.0,
            "target_workspace": target_workspace.0,
            "follow": plan.follow,
        }),
        LifecycleOperation::MoveDeferred {
            window,
            source_output,
            source_workspace,
            target_output,
            target_workspace,
        } => serde_json::json!({
            "op": "move-deferred",
            "window": window.0,
            "source_output": source_output.0,
            "source_workspace": source_workspace.0,
            "target_output": target_output.0,
            "target_workspace": target_workspace.0,
            "follow": plan.follow,
        }),
        _ => unreachable!("SendWorkspacePlan always carries a send move operation"),
    };
    // Same binding posture as workspace send: the wire token binds the reply
    // for exact matching, not as a native-verification or
    // committed-native-success assertion. The adapter reconciles both domains
    // after the immediate planned-topology commit.
    let preconditions: Vec<&'static str> = plan
        .preconditions
        .iter()
        .map(|p| lifecycle_precondition_str(*p))
        .collect();
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "planned",
        kind: Some("send-to-output".to_owned()),
        message: None,
        base_revision: Some(plan.base_revision),
        detail: Some(serde_json::json!({
            "kind": "send-to-output",
            "policy_version": plan.policy_version,
            "capability": "move-tiled",
        })),
        desired_geometry: Some(plan.geometry.iter().map(geometry_reply).collect()),
        desired_focus: match (&plan.focus_domain, &plan.focus_leaf) {
            (Some(d), Some(l)) => Some(focus_reply(d, l)),
            _ => None,
        },
        float_geometry: None,
        preconditions: Some(preconditions),
        operation: Some(operation_value),
        preview_rect: None,
        hover_prior: None,
    })
}

/// Whole-workspace migration serializer driven by a
/// [`tiler_core::boundary::MigrateWorkspacePlan`] (single source). The
/// operation binds the frozen source/target identity plus the carried active
/// client (`active_window`, null when the request carried none) the adapter
/// must gate setters and follow/completion on. `detail.commit` names the
/// retained effect (`retained-rekey-planned`); native completion is never
/// claimed here.
fn serialize_migrate_workspace_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::MigrateWorkspacePlan,
) -> String {
    let operation_value = serde_json::json!({
        "op": "migrate-workspace",
        "direction": direction_str(plan.direction),
        "source_output": plan.source.output.0,
        "source_workspace": plan.source.workspace.0,
        "target_output": plan.target.output.0,
        "target_workspace": plan.target.workspace.0,
        "active_window": plan.active_window.as_ref().map(|window| window.0.clone()),
    });
    let preconditions: Vec<&'static str> = plan
        .preconditions
        .iter()
        .map(|p| lifecycle_precondition_str(*p))
        .collect();
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "planned",
        kind: Some("migrate-workspace".to_owned()),
        message: None,
        base_revision: Some(plan.base_revision),
        detail: Some(serde_json::json!({
            "kind": "migrate-workspace",
            "policy_version": tiler_core::contract::LIFECYCLE_POLICY_VERSION,
            "capability": "migrate-workspace",
            "commit": "retained-rekey-planned",
            "direction": direction_str(plan.direction),
            "source_output": plan.source.output.0,
            "source_workspace": plan.source.workspace.0,
            "target_output": plan.target.output.0,
            "target_workspace": plan.target.workspace.0,
            "active_window": plan.active_window.as_ref().map(|window| window.0.clone()),
            "members": plan.members,
            "floats": plan.floats,
        })),
        desired_geometry: Some(plan.geometry.iter().map(geometry_reply).collect()),
        desired_focus: match (&plan.focus_domain, &plan.focus_leaf) {
            (Some(d), Some(l)) => Some(focus_reply(d, l)),
            _ => None,
        },
        float_geometry: None,
        preconditions: Some(preconditions),
        operation: Some(operation_value),
        preview_rect: None,
        hover_prior: None,
    })
}

/// Byte-exact read-only drag-preview serializer driven by a
/// [`tiler_core::boundary::DragPreviewPlan`] (single source). Outcome
/// `preview` with kind `drag-preview`: the proposed source rectangle plus the
/// carried hover state for the next preview or the final drop. No geometry
/// commit, no focus change, no preconditions/operation: the adapter routes
/// this to the translucent overlay and forwards `hover_prior` verbatim.
fn serialize_drag_preview_reply(
    correlation_id: &str,
    plan: &tiler_core::boundary::DragPreviewPlan,
) -> String {
    let preview = &plan.preview;
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "preview",
        kind: Some("drag-preview".to_owned()),
        message: None,
        base_revision: Some(plan.base_revision),
        detail: Some(serde_json::json!({
            "kind": "drag-preview",
            "capability": "place-tiled",
            "side": preview.side.as_str(),
            "axis": match preview.axis {
                tiler_core::directional::Axis::Horizontal => "horizontal",
                tiler_core::directional::Axis::Vertical => "vertical",
            },
            "before": preview.before,
            "wrap": preview.wrap,
            "target_group": preview.target_group.0,
            "insertion_index": preview.insertion_index,
            "target_leaf": preview.target_leaf.0,
            "target_window": preview.target_window.0,
            "source_leaf": preview.source_leaf.0,
            "source_window": preview.source_window.0,
        })),
        desired_geometry: None,
        desired_focus: None,
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: Some(RectDto {
            x: preview.proposed_rect.x,
            y: preview.proposed_rect.y,
            w: preview.proposed_rect.w,
            h: preview.proposed_rect.h,
        }),
        hover_prior: Some(serde_json::json!({
            "domain_output": preview.domain.output.0,
            "domain_workspace": preview.domain.workspace.0,
            "source_leaf": preview.source_leaf.0,
            "source_window": preview.source_window.0,
            "revision": preview.revision,
            "prior": preview.prior.as_ref().map(|prior| serde_json::json!({
                "group": prior.group.0,
                "edge": prior.edge.as_str(),
            })).unwrap_or(serde_json::Value::Null),
        })),
    })
}

/// Byte-exact explicit release serializer: outcome `released` with kind
/// `release-domain`. No base revision (the slot is gone), no geometry, no
/// focus, no preconditions/operation: the adapter must perform zero native
/// writes and only drop that domain's applied evidence. A later ordinary
/// fresh reconcile re-adopts current geometry.
fn serialize_release_reply(correlation_id: &str) -> String {
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "released",
        kind: Some("release-domain".to_owned()),
        message: None,
        base_revision: None,
        detail: Some(serde_json::json!({
            "kind": "release-domain",
        })),
        desired_geometry: None,
        desired_focus: None,
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

/// Shared typed-reply choke point: every [`tiler_core::boundary::CoreReply`]
/// variant serializes here through the exact legacy wire shapes, so output
/// stays byte identical. Rejection/status/ack/commit/cancel arms reuse their
/// existing tiny serializers (single source); transaction orchestration
/// itself never crosses.
fn serialize_core_reply(ctx: &Validated, reply: &tiler_core::boundary::CoreReply) -> String {
    use tiler_core::boundary::CoreReply;
    let cid = ctx.request.correlation_id.clone();
    match reply {
        CoreReply::Projection(plan) => planned_projection_reply(&cid, plan),
        CoreReply::Tiled(plan) => planned_tiled_reply(&cid, plan),
        CoreReply::SendWorkspace(plan) => serialize_send_workspace_reply(&cid, plan),
        CoreReply::SendOutput(plan) => serialize_send_output_reply(&cid, plan),
        CoreReply::MigrateWorkspace(plan) => serialize_migrate_workspace_reply(&cid, plan),
        CoreReply::MoveDirectional(plan) => serialize_move_reply(&cid, plan),
        CoreReply::FocusDirectional(plan) => serialize_focus_reply(&cid, plan),
        CoreReply::Resize(plan) => serialize_resize_reply(&cid, plan),
        CoreReply::ActiveGroup(found) => serialize_active_group_found(ctx, found),
        CoreReply::DragPreview(plan) => serialize_drag_preview_reply(&cid, plan),
        CoreReply::Released => serialize_release_reply(&cid),
        CoreReply::NoGroup {
            base_revision,
            reason,
        } => no_group_reply(ctx, *base_revision, reason.as_str()),
        CoreReply::Rejected { kind, message } => rejected(cid, kind, message),
        CoreReply::SnapshotInvalid { message, detail } => snapshot_invalid(cid, message, detail),
        CoreReply::Diverged(reason) => diverged_reply(&cid, *reason),
    }
}

/// Convert carried wire windows to the portable near-strip fitter.
/// AR12 hints map straight across (raw values; meaningfulness is judged in
/// [`tiler_core::size_hints`] so absurd reports behave as absent).
fn engine_window_from_dto(entry: &ObservedDto) -> tiler_core::seed::EngineWindow {
    tiler_core::seed::EngineWindow {
        window: WindowId(entry.window.clone()),
        output: OutputId(entry.output.clone()),
        workspace: WorkspaceId(entry.workspace.clone()),
        rect: Rect {
            x: entry.rect.x,
            y: entry.rect.y,
            w: entry.rect.w,
            h: entry.rect.h,
        },
        floating: entry.floating,
        fit_excluded: entry.fit_excluded,
        fullscreen: entry.fullscreen,
        maximized: entry.maximized,
        sticky: entry.sticky,
        fixed_auto: entry.fixed_auto,
        fixed_suppress: entry.fixed_suppress,
        hints: tiler_core::size_hints::WindowSizeHints {
            min_w: entry.min_size.as_ref().map(|size| size.w),
            min_h: entry.min_size.as_ref().map(|size| size.h),
            max_w: entry.max_size.as_ref().map(|size| size.w),
            max_h: entry.max_size.as_ref().map(|size| size.h),
        },
    }
}

/// Validated workspace-send route input: the target domain plus the exact
/// mover binding. The source domain is the already-validated request domain.
#[derive(Debug)]
struct WorkspaceInput {
    target_domain: OutputDomain,
    target_key: DomainKey,
    window: WindowId,
    follow: bool,
}

/// Stable lifecycle precondition token (mirrors the KWin wire tokens).
#[must_use]
fn lifecycle_precondition_str(value: LifecyclePrecondition) -> &'static str {
    match value {
        LifecyclePrecondition::WindowObserved => "window-observed",
        LifecyclePrecondition::DesiredTopologyValid => "desired-topology-valid",
        LifecyclePrecondition::AdapterMustVerifyPostconditions => {
            "adapter-must-verify-postconditions"
        }
    }
}

/// Terminal divergence reply: outcome `diverged`, exact bounded kind.
fn diverged_reply(correlation_id: &str, reason: tiler_core::contract::DivergenceKind) -> String {
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: correlation_id.to_owned(),
        outcome: "diverged",
        kind: Some(reason.as_str().to_owned()),
        message: Some(reason.message().to_owned()),
        base_revision: None,
        detail: None,
        desired_geometry: None,
        desired_focus: None,
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

/// Authoritative live-tree planner.
///
/// Retains one committed [`Session`] per logical domain across `DescribePlan`
/// calls. Observations validate membership/divergence against the retained
/// topology but never rebuild it: a known domain proposes directly, so equal
/// non-focused rectangles cannot force `ambiguous-placement` after the initial
/// plan. Domain bounds changes reproject the retained tree when the domain key
/// and complete window set remain unchanged; owner/generation change (adapter
/// restart), terminal divergence, pending residue, or membership divergence
/// (`Diverged`/`partial-observation`) discard that domain's retained state and
/// rebuild once via the [`tiler_core::seed::seed_session`] path. If the rebuild cannot safely
/// infer topology (`ambiguous-placement`), reject rather than wedge. Each
/// successful plan commits synchronously in the same call, so no pending
/// crosses calls and no stale data crosses domains/owner/generation.
///
/// Standalone workspace-send route: `send-to-workspace` commits synchronously
/// through the Engine with native assignment plus both-domain geometry.
///
/// Directional R4 route: `move` with a two-domain payload proposes an R4
/// cross-output transfer synchronously through the Engine. R1-R3 stay
/// synchronous.
#[derive(Debug, Default)]
pub struct Planner {
    engine: Engine,
    preview_stage_gate: PreviewStageDedup,
}

impl Planner {
    /// Empty retained planner with opt-in Q2 fixed-size float admission
    /// enabled on the Linux route (D1-D8). Windows carriers use
    /// `Engine::new` directly and keep the opt-in off.
    ///
    /// Contract: `Default` is the inert baseline (opt-in off, pre-Q2
    /// behavior) while `new()` is the Linux route (opt-in on). No
    /// production caller uses `Default`; it exists only for the derive
    /// and must stay inert so default-constructed planners never
    /// classify.
    #[must_use]
    pub fn new() -> Self {
        let mut planner = Self::default();
        planner.engine.set_fixed_size_admission(true);
        // G-06 maximized directional focus fence (REQ-MAX-08): Linux route
        // only. Windows carriers keep `Engine::new` defaults (fence off).
        planner.engine.set_maximized_focus_fence(true);
        planner
    }

    /// Number of retained domains.
    #[must_use]
    pub fn retained_domains(&self) -> usize {
        self.engine.retained_domains()
    }

    /// Retained owner binding, if any.
    #[must_use]
    pub fn owner(&self) -> Option<&OwnerId> {
        self.engine.owner()
    }

    /// Retained generation binding, if any.
    #[must_use]
    pub fn generation(&self) -> Option<&GenerationId> {
        self.engine.generation()
    }

    /// Trace-only placement lines for the last evaluated op: at most one
    /// startup-fit line plus at most one send-placement line. Empty when the
    /// last op recorded no fresh/send decision. Pure read of the retained
    /// engine; the `handle_and_serialize` choke point already emits these
    /// under the opt-in trace gate.
    #[must_use]
    pub fn placement_trace_lines(&self) -> Vec<String> {
        placement_trace_lines(&self.engine)
    }

    fn sync_binding(&mut self, owner: &OwnerId, generation: &GenerationId) {
        self.engine.sync_binding(owner, generation);
    }

    /// Stateful evaluation across calls. Retained reconcile accepts work-area
    /// bounds changes only when the domain key and complete window set remain
    /// unchanged, projecting the existing tree without replacing shares or
    /// topology.
    pub fn evaluate(&mut self, request_json: &str) -> String {
        let ctx = match validate_request(request_json) {
            Ok(ctx) => ctx,
            Err(reply) => {
                // Validation failures are cold-path: parse the correlation
                // inline and flush any pending stage run so counts never
                // strand across the boundary.
                let correlation = serde_json::from_str::<serde_json::Value>(request_json)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("correlation_id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                for line in self.preview_stage_gate.reset(&correlation) {
                    use std::io::Write;
                    let _ = writeln!(std::io::stderr(), "{line}");
                }
                return reply;
            }
        };
        // R-SPC-04 D1 predicate: validated above, so this parse is total.
        // Applied to the retained engine for subsequent admissions only;
        // existing automatic/override marks are untouched (D2).
        if let Some(predicate) = tiler_core::size_hints::FixedSizePredicate::parse_wire(
            ctx.request.fixed_size_predicate.as_str(),
        ) {
            self.engine.set_fixed_size_predicate(predicate);
        }
        if validated_op(&ctx).as_str() == "send-to-workspace" {
            return self.evaluate_workspace_request(&ctx);
        }
        if validated_op(&ctx).as_str() == "send-to-output" {
            return self.evaluate_send_output_request(&ctx);
        }
        if validated_op(&ctx).as_str() == "migrate-workspace" {
            return self.evaluate_migrate_workspace_request(&ctx);
        }
        self.sync_binding(&ctx.owner, &ctx.generation);
        // Typed codec: reconcile/update-gaps/active-group
        // parse once via `SyncCommand` after all boundaries (validation,
        // send dispatch, binding sync) and call the
        // inner bodies directly, eliminating the second `from_value` + op-string
        // check on this production path. The string guard preserves exact
        // unknown/missing/non-string `unknown-value` behavior without a typed
        // parse.
        // Move/focus/resize/pointer-resize/toggle-float/drag-drop/drag-preview
        // parse `SyncCommand` once in place inside their handlers (see `SyncCommand`
        // docs for the exact probe/ordering reasons), so these arms dispatch
        // by op string.
        match validated_op(&ctx).as_str() {
            "reconcile" | "update-gaps" | "active-group" | "release-domain" => {
                match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
                    Ok(SyncCommand::Reconcile {}) => self.evaluate_reconcile_inner(&ctx),
                    Ok(SyncCommand::UpdateGaps {}) => self.evaluate_update_gaps_inner(&ctx),
                    Ok(SyncCommand::ReleaseDomain {}) => self.evaluate_release_inner(&ctx),
                    Ok(command @ SyncCommand::ActiveGroup {}) => {
                        // Production typed-boundary route: convert the
                        // already-decoded command after all fences, then run
                        // the typed active-group body (no second parse).
                        let core_command =
                            core_command_from_sync(&command).expect("non-verify sync op converts");
                        self.evaluate_active_group_typed(&ctx, &core_command)
                    }
                    // Unreachable: the outer string guard admits only the three
                    // ops above, so no other variant can decode here.
                    Ok(_) => rejected(
                        valid_correlation_echo(&ctx.raw),
                        "unknown-value",
                        MSG_UNKNOWN_VALUE,
                    ),
                    Err(error) => {
                        let (kind, message) = classify_parse_error(&error);
                        rejected(valid_correlation_echo(&ctx.raw), kind, message)
                    }
                }
            }
            "move" => self.evaluate_move_retained(&ctx),
            "focus" => self.evaluate_focus_retained(&ctx),
            "resize" => self.evaluate_resize_retained(&ctx),
            "pointer-resize" => self.evaluate_pointer_resize_retained(&ctx),
            "toggle-float" => self.evaluate_toggle_float_retained(&ctx),
            "toggle-orientation" => self.evaluate_toggle_orientation_retained(&ctx),
            "drag-drop" => self.evaluate_drag_retained(&ctx, false),
            "drag-preview" => self.evaluate_drag_retained(&ctx, true),
            _ => rejected(
                valid_correlation_echo(&ctx.raw),
                "unknown-value",
                MSG_UNKNOWN_VALUE,
            ),
        }
    }

    /// Engine-handle choke point: runs the owned [`Engine::handle`] entry
    /// point, emits the bounded correlated convergence summary when the op
    /// converged with nonzero counts plus the fresh adoption-fit summary
    /// when the op attempted a fresh adoption, then serializes through the
    /// typed choke point. Reply bytes are unchanged; summaries carry counts
    /// plus reason tokens only (no new reply field, no identifiers, no
    /// payloads).
    fn handle_and_serialize(
        &mut self,
        ctx: &Validated,
        event: &tiler_core::boundary::CoreEvent,
    ) -> String {
        let reply = self.engine.handle(event);
        emit_engine_convergence(&self.engine);
        emit_engine_adoption_fit(&self.engine);
        emit_engine_fixed_admission(&self.engine);
        emit_engine_migration(&self.engine);
        // Transition-gated preview stages: identical per-sample repeats
        // stay silent with a counter; a stage change or a success flushes
        // the run. Same stderr policy as the other emit fns (no locks).
        let staged: Option<(String, &'static str)> = self
            .engine
            .last_drag_preview_stage()
            .map(|report| (report.correlation.as_str().to_owned(), report.stage));
        let lines = match staged {
            Some((correlation, stage)) => self.preview_stage_gate.observe(&correlation, stage),
            None => self.preview_stage_gate.reset(&ctx.request.correlation_id),
        };
        for line in lines {
            use std::io::Write;
            let _ = writeln!(std::io::stderr(), "{line}");
        }
        emit_engine_placement_trace(&self.engine);
        serialize_core_reply(ctx, &reply)
    }

    fn evaluate_toggle_float_retained(&mut self, ctx: &Validated) -> String {
        // Probe-before-parse precedence for malformed floats on untracked
        // floating windows (`not-tiled`, never `unknown-field` or
        // `float-rect-invalid`): only a fully valid toggle-float command is
        // pre-rejected here. A valid toggle-float reaches the Engine, which
        // converges a fresh floating observation into an exception and
        // unfloats it into the current domain.
        let well_formed = match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
            Ok(SyncCommand::ToggleFloat { window, float_rect }) => {
                is_opaque_id(&window)
                    && ctx
                        .request
                        .windows
                        .iter()
                        .any(|entry| entry.window == window)
                    && float_rect
                        .as_ref()
                        .is_none_or(|rect| valid_carried_rect(rect.x, rect.y, rect.w, rect.h))
            }
            _ => false,
        };
        if !well_formed
            && ctx
                .request
                .windows
                .iter()
                .find(|entry| {
                    entry.window
                        == ctx
                            .request
                            .command
                            .get("window")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                })
                .is_some_and(|entry| entry.floating)
            && !self.engine.contains(&ctx.domain_key)
        {
            return rejected(
                ctx.request.correlation_id.clone(),
                RefusalKind::NotTiled.as_str(),
                RefusalKind::NotTiled.message(),
            );
        }
        evaluate_toggle_float_with(ctx, |window, float_rect| {
            // Engine-owned toggle-float orchestration: the validated window
            // and float rect cross in the typed command; seed ordering,
            // relocation, propose/commit, and store run in `Engine::handle`.
            let core_command = tiler_core::boundary::CoreCommand::ToggleFloat {
                window: window.to_owned(),
                float_rect,
            };
            let event = core_event(ctx, &core_command);
            self.handle_and_serialize(ctx, &event)
        })
    }

    fn evaluate_toggle_orientation_retained(&mut self, ctx: &Validated) -> String {
        // Strict tagged decode in place (see `SyncCommand`): a present-but-
        // wrong op surfaces as `toggle-orient-op-invalid`, mirroring
        // the move/focus/resize handlers. No floating probe: the Engine
        // converges fresh floating observations into exceptions and the
        // session refuses them as `not-tiled`, exactly like the move route.
        // `domains` payloads never reach here: validation already refused
        // them on every non-focus/move op, so this stays single-domain.
        let window = match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
            Ok(SyncCommand::ToggleOrientation { window }) => window,
            Ok(_) => {
                return snapshot_invalid(
                    ctx.request.correlation_id.clone(),
                    MSG_OBSERVATION,
                    "toggle-orient-op-invalid",
                );
            }
            Err(error) => {
                if is_unknown_variant(&error) {
                    return snapshot_invalid(
                        ctx.request.correlation_id.clone(),
                        MSG_OBSERVATION,
                        "toggle-orient-op-invalid",
                    );
                }
                let (kind, message) = classify_parse_error(&error);
                return rejected(valid_correlation_echo(&ctx.raw), kind, message);
            }
        };
        if !is_opaque_id(&window) {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "toggle-orient-window-invalid",
            );
        }
        // Engine-owned orchestration: the validated window crosses in the
        // typed command; seed ordering, focus sync, relocation,
        // propose/commit, and store run in `Engine::handle`. Serialization
        // funnels through the typed choke point.
        let core_command = tiler_core::boundary::CoreCommand::ToggleOrientation {
            window: window.clone(),
        };
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    fn evaluate_move_retained(&mut self, ctx: &Validated) -> String {
        // Strict tagged decode in place (see `SyncCommand`): the directional
        // pair branch and every validation below run on the rebuilt plumbing
        // value exactly as before.
        let command: DirectedCommand =
            match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
                Ok(SyncCommand::Move {
                    window,
                    direction,
                    cross_output_transfer,
                    same_axis_move,
                }) => {
                    let Some(same_axis_move) = SameAxisMove::parse_wire(&same_axis_move) else {
                        return snapshot_invalid(
                            ctx.request.correlation_id.clone(),
                            MSG_UNKNOWN_VALUE,
                            "move-op-invalid",
                        );
                    };
                    DirectedCommand {
                        window,
                        direction,
                        cross_output_transfer,
                        same_axis_move,
                        float_subject: false,
                    }
                }
                Ok(_) => {
                    return snapshot_invalid(
                        ctx.request.correlation_id.clone(),
                        MSG_OPAQUE_ID,
                        "move-op-invalid",
                    );
                }
                Err(error) => {
                    if is_unknown_variant(&error) {
                        return snapshot_invalid(
                            ctx.request.correlation_id.clone(),
                            MSG_OPAQUE_ID,
                            "move-op-invalid",
                        );
                    }
                    let (kind, message) = classify_parse_error(&error);
                    return rejected(valid_correlation_echo(&ctx.raw), kind, message);
                }
            };
        // Production directional route: two-domain observations build one
        // temporary pair from canonical retained domain sessions.
        // Single-domain legacy requests run the Engine-owned local path below.
        if directional_pair(ctx).is_some() {
            return self.evaluate_move_directional(ctx, &command);
        }
        if !is_opaque_id(&command.window) {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "move-window-invalid",
            );
        }
        if parse_direction(&command.direction).is_none() {
            return rejected(
                ctx.request.correlation_id.clone(),
                "direction-invalid",
                MSG_DIRECTION,
            );
        }
        // Engine-owned local orchestration: the validated window/direction
        // cross opaquely in the typed command; seed ordering, focus sync,
        // relocation, propose/commit, and store run in `Engine::handle`.
        // Serialization funnels through the typed choke point.
        let core_command = tiler_core::boundary::CoreCommand::Move {
            window: command.window.clone(),
            direction: command.direction.clone(),
            cross_output_transfer: command.cross_output_transfer,
            same_axis_move: command.same_axis_move,
        };
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    fn evaluate_focus_retained(&mut self, ctx: &Validated) -> String {
        // Strict tagged decode in place (see `SyncCommand`): same contract as
        // `evaluate_move_retained`; the `cross_output_transfer` carrier is
        // preserved so explicit values keep parsing exactly as before.
        let command: DirectedCommand =
            match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
                Ok(SyncCommand::Focus {
                    window,
                    direction,
                    cross_output_transfer,
                    float_subject,
                }) => DirectedCommand {
                    window,
                    direction,
                    cross_output_transfer,
                    // Focus never reads the move-only mode; the default keeps
                    // the value total without touching the focus wire shape.
                    same_axis_move: SameAxisMove::GroupWithNeighbor,
                    float_subject,
                },
                Ok(_) => {
                    return snapshot_invalid(
                        ctx.request.correlation_id.clone(),
                        MSG_OPAQUE_ID,
                        "focus-op-invalid",
                    );
                }
                Err(error) => {
                    if is_unknown_variant(&error) {
                        return snapshot_invalid(
                            ctx.request.correlation_id.clone(),
                            MSG_OPAQUE_ID,
                            "focus-op-invalid",
                        );
                    }
                    let (kind, message) = classify_parse_error(&error);
                    return rejected(valid_correlation_echo(&ctx.raw), kind, message);
                }
            };
        // Production directional route: two-domain observations try local
        // focus first, then the exhausted Left/Right cross-output proposal
        // against a temporary pair built from canonical domain state.
        // Single-domain legacy requests run the Engine-owned local path below.
        if directional_pair(ctx).is_some() {
            return self.evaluate_focus_directional(ctx, &command);
        }
        if !is_opaque_id(&command.window) {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "focus-window-invalid",
            );
        }
        if parse_direction(&command.direction).is_none() {
            return rejected(
                ctx.request.correlation_id.clone(),
                "direction-invalid",
                MSG_DIRECTION,
            );
        }
        // Float-origin subjects never take the single-domain local route
        // (the adapter retains Up/Down and no-adjacent misses without
        // dispatch); a flagged single-domain request is misuse and refuses
        // before the Engine. Tile-origin requests are unaffected.
        if command.float_subject {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OBSERVATION,
                "domain-invalid",
            );
        }
        // Engine-owned local orchestration: the validated window/direction
        // cross opaquely in the typed command; seed ordering, focus sync,
        // relocation, propose/commit, and store run in `Engine::handle`
        // (local only, no cross fallback). Serialization funnels through the
        // typed choke point.
        let core_command = tiler_core::boundary::CoreCommand::Focus {
            window: command.window.clone(),
            direction: command.direction.clone(),
            cross_output_transfer: command.cross_output_transfer,
            float_subject: false,
        };
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    /// Production directional move: envelope, tagged decoding, pair scope
    /// shape, mover binding, and parsed-direction/op validation stay here at
    /// their exact positions; planning, R4 staging, and the R1-R3 synchronous
    /// commit are the Engine-owned [`tiler_core::engine::Engine::handle`]
    /// typed entry point over the validated pair. Serialization funnels
    /// through the typed [`serialize_core_reply`] choke point.
    fn evaluate_move_directional(&mut self, ctx: &Validated, command: &DirectedCommand) -> String {
        let cid = ctx.request.correlation_id.clone();
        if !is_opaque_id(&command.window) {
            return snapshot_invalid(cid, MSG_OPAQUE_ID, "move-window-invalid");
        }
        if parse_direction(&command.direction).is_none() {
            return rejected(cid, "direction-invalid", MSG_DIRECTION);
        }
        if directional_pair(ctx).is_none() {
            return snapshot_invalid(cid, MSG_OBSERVATION, "domain-invalid");
        }
        let core_command = core_command_from_sync(&SyncCommand::Move {
            window: command.window.clone(),
            direction: command.direction.clone(),
            cross_output_transfer: command.cross_output_transfer,
            same_axis_move: command.same_axis_move.as_wire_str().to_owned(),
        })
        .expect("move sync op converts");
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    /// Production directional focus: envelope, tagged decoding, pair scope
    /// shape, and parsed-direction/op validation stay here at their exact
    /// positions; planning and the synchronous commit are the Engine-owned
    /// [`tiler_core::engine::Engine::handle`] typed entry point. Exactly one
    /// focus actuation downstream; no geometry/layout/window membership
    /// writes.
    fn evaluate_focus_directional(&mut self, ctx: &Validated, command: &DirectedCommand) -> String {
        let cid = ctx.request.correlation_id.clone();
        if !is_opaque_id(&command.window) {
            return snapshot_invalid(cid, MSG_OPAQUE_ID, "focus-window-invalid");
        }
        if parse_direction(&command.direction).is_none() {
            return rejected(cid, "direction-invalid", MSG_DIRECTION);
        }
        if directional_pair(ctx).is_none() {
            return snapshot_invalid(cid, MSG_OBSERVATION, "domain-invalid");
        }
        // Float-origin misuse fails closed: the flag rides Left/Right
        // two-domain requests only (adapter retains Up/Down misses without
        // dispatch, and single-domain flagged requests refuse above).
        if command.float_subject && !matches!(command.direction.as_str(), "left" | "right") {
            return snapshot_invalid(cid, MSG_OPAQUE_ID, "focus-op-invalid");
        }
        let core_command = core_command_from_sync(&SyncCommand::Focus {
            window: command.window.clone(),
            direction: command.direction.clone(),
            cross_output_transfer: command.cross_output_transfer,
            float_subject: command.float_subject,
        })
        .expect("focus sync op converts");
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    fn evaluate_resize_retained(&mut self, ctx: &Validated) -> String {
        // Strict tagged decode in place (see `SyncCommand`).
        let (window_raw, direction_raw, mode_raw, press_index) =
            match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
                Ok(SyncCommand::Resize {
                    window,
                    direction,
                    mode,
                    press_index,
                }) => (window, direction, mode, press_index),
                Ok(_) => {
                    return snapshot_invalid(
                        ctx.request.correlation_id.clone(),
                        MSG_OPAQUE_ID,
                        "resize-op-invalid",
                    );
                }
                Err(error) => {
                    if is_unknown_variant(&error) {
                        return snapshot_invalid(
                            ctx.request.correlation_id.clone(),
                            MSG_OPAQUE_ID,
                            "resize-op-invalid",
                        );
                    }
                    let (kind, message) = classify_parse_error(&error);
                    return rejected(valid_correlation_echo(&ctx.raw), kind, message);
                }
            };
        if !is_opaque_id(&window_raw) {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "resize-window-invalid",
            );
        }
        if parse_direction(&direction_raw).is_none() {
            return rejected(
                ctx.request.correlation_id.clone(),
                "direction-invalid",
                MSG_DIRECTION,
            );
        }
        if parse_mode(&mode_raw).is_none() {
            return rejected(
                ctx.request.correlation_id.clone(),
                "direction-invalid",
                MSG_DIRECTION,
            );
        }
        // Engine-owned keyboard-resize orchestration: the validated
        // window/direction/mode cross opaquely in the typed command;
        // seed ordering, focus sync, keyboard-gated propose/commit, and
        // store run in `Engine::handle`. Serialization funnels through the
        // typed choke point.
        let core_command = tiler_core::boundary::CoreCommand::Resize {
            window: window_raw,
            direction: direction_raw,
            mode: mode_raw,
            press_index,
        };
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    fn evaluate_pointer_resize_retained(&mut self, ctx: &Validated) -> String {
        // Strict tagged decode in place (see `SyncCommand`).
        let (window_raw, direction_raw, boundary, direction2_raw, boundary2) =
            match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
                Ok(SyncCommand::PointerResize {
                    window,
                    direction,
                    boundary,
                    direction2,
                    boundary2,
                }) => (window, direction, boundary, direction2, boundary2),
                Ok(_) => {
                    return snapshot_invalid(
                        ctx.request.correlation_id.clone(),
                        MSG_OPAQUE_ID,
                        "pointer-resize-op-invalid",
                    );
                }
                Err(error) => {
                    if is_unknown_variant(&error) {
                        return snapshot_invalid(
                            ctx.request.correlation_id.clone(),
                            MSG_OPAQUE_ID,
                            "pointer-resize-op-invalid",
                        );
                    }
                    let (kind, message) = classify_parse_error(&error);
                    return rejected(valid_correlation_echo(&ctx.raw), kind, message);
                }
            };
        if !is_opaque_id(&window_raw) {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "pointer-resize-window-invalid",
            );
        }
        if parse_direction(&direction_raw).is_none() {
            return rejected(
                ctx.request.correlation_id.clone(),
                "direction-invalid",
                MSG_DIRECTION,
            );
        }
        // Corner second axis is both-or-neither; a half-present pair is an
        // op shape violation, and an unparsable second direction binds the
        // exact direction refusal like the primary.
        if direction2_raw.is_some() != boundary2.is_some() {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "pointer-resize-op-invalid",
            );
        }
        if let Some(second) = &direction2_raw
            && parse_direction(second).is_none()
        {
            return rejected(
                ctx.request.correlation_id.clone(),
                "direction-invalid",
                MSG_DIRECTION,
            );
        }
        // Engine-owned pointer-resize orchestration: the validated
        // window/direction plus the opaquely carried boundary (and the
        // optional corner second axis) cross in the typed command; seed
        // ordering, focus sync, pointer-gated propose/commit, and store run
        // in `Engine::handle`. Serialization funnels through the typed
        // choke point.
        let core_command = tiler_core::boundary::CoreCommand::PointerResize {
            window: window_raw,
            direction: direction_raw,
            boundary,
            direction2: direction2_raw,
            boundary2,
        };
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    /// Shared drag-drop/preview body (option B): one `begin_drag` over the same
    /// complete observation (including size hints); preview runs it on a
    /// discarded working clone and replies read-only, drop commits with
    /// immediate acknowledge/verify. Strict tagged decode keeps distinct
    /// `drag-drop-op-invalid` / `drag-preview-op-invalid` tokens; the Engine
    /// window fence keeps the shared `drag-drop-window-invalid` detail; wire-shape
    /// violations keep `classify_parse_error` behavior. Pointer `x`/`y` cross
    /// as decoded (outside points refuse in the Engine as bounded `unchanged`);
    /// an invalid hover carry or source binding degrades advisory-style.
    /// Preview alone refuses a carried workspace-send target
    /// (`cross-domain-mismatch`); a carried `domains` payload already refused
    /// at validation for both.
    fn evaluate_drag_retained(&mut self, ctx: &Validated, is_preview: bool) -> String {
        if is_preview
            && (!ctx.request.target_windows.is_empty() || ctx.request.target_domain.is_some())
        {
            return rejected(
                ctx.request.correlation_id.clone(),
                "cross-domain-mismatch",
                MSG_CROSS_DOMAIN,
            );
        }
        let op_invalid = if is_preview {
            "drag-preview-op-invalid"
        } else {
            "drag-drop-op-invalid"
        };
        let command = match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
            Ok(command @ SyncCommand::DragDrop(_)) if !is_preview => command,
            Ok(command @ SyncCommand::DragPreview(_)) if is_preview => command,
            Ok(_) => {
                return snapshot_invalid(
                    ctx.request.correlation_id.clone(),
                    MSG_OPAQUE_ID,
                    op_invalid,
                );
            }
            Err(error) => {
                if is_unknown_variant(&error) {
                    return snapshot_invalid(
                        ctx.request.correlation_id.clone(),
                        MSG_OPAQUE_ID,
                        op_invalid,
                    );
                }
                let (kind, message) = classify_parse_error(&error);
                return rejected(valid_correlation_echo(&ctx.raw), kind, message);
            }
        };
        let core_command = core_command_from_sync(&command).expect("drag sync op converts");
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    /// Direct-evaluator compatibility wrapper (test-only): exact legacy
    /// `from_value` + op-check behavior. Production `evaluate` bypasses this
    /// via the typed [`SyncCommand`] single parse + inner below.
    #[cfg(test)]
    fn evaluate_reconcile_retained(&mut self, ctx: &Validated) -> String {
        let command: ReconcileCommand = match serde_json::from_value(ctx.request.command.clone()) {
            Ok(command) => command,
            Err(error) => {
                let (kind, message) = classify_parse_error(&error);
                return rejected(valid_correlation_echo(&ctx.raw), kind, message);
            }
        };
        if command.op != "reconcile" {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "reconcile-op-invalid",
            );
        }
        self.evaluate_reconcile_inner(ctx)
    }

    /// Production reconcile body without a second command parse/op check.
    /// The `evaluate` typed path parses [`SyncCommand`] once after all
    /// dispatch boundaries and calls here directly; the retained wrapper
    /// above preserves the exact direct-evaluator invalid-op behavior.
    fn evaluate_reconcile_inner(&mut self, ctx: &Validated) -> String {
        // Engine-owned reconcile orchestration: relocation, fences,
        // membership, projection, and reprojection run in `Engine::handle`.
        // Serialization funnels through the typed choke point.
        let core_command = tiler_core::boundary::CoreCommand::Reconcile;
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    /// Production release-domain body without a second command parse/op check.
    /// Same boundary contract as [`Self::evaluate_reconcile_inner`]: the
    /// envelope is already validated (that domain's complete current
    /// observation, same homing/rect/focus rules as reconcile). The Engine
    /// drops the exact domain slot with no geometry; the reply carries none,
    /// so the adapter must perform zero native writes.
    fn evaluate_release_inner(&mut self, ctx: &Validated) -> String {
        let core_command = tiler_core::boundary::CoreCommand::ReleaseDomain;
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    /// Deliberate retained gap-update reprojection for the interim tiler
    /// reload: adopt the carried inner/outer gaps (plus the carried
    /// outer-inset bounds they imply) and reproject the retained tree with
    /// the new inner gap, preserving topology, shares, membership, focus,
    /// exceptions, and accepted revision. No pending is staged: like
    /// reconcile this is a pure projection whose reply the adapter applies
    /// natively. Fail-closed without mutation on unknown/diverged/pending
    /// domains, membership or focus mismatch, unprojectable results, or an
    /// unadoptable gap/bounds update. Never seeds, relocates, resets, or
    /// reseeds a session: an unknown domain refuses so the normal reconcile path
    /// seeds it with the new gaps instead. A simultaneous work-area change
    /// folds into the same projection with reconcile-equivalent safety; a
    /// simultaneous membership change refuses as partial-observation and the
    /// normal reconcile path owns it.
    /// Production update-gaps body without a second command parse/op check.
    /// Same boundary contract as [`Self::evaluate_reconcile_inner`].
    fn evaluate_update_gaps_inner(&mut self, ctx: &Validated) -> String {
        // Engine-owned update-gaps orchestration: fences, membership,
        // projection, and gap adoption run in `Engine::handle`.
        // Serialization funnels through the typed choke point.
        let core_command = tiler_core::boundary::CoreCommand::UpdateGaps;
        let event = core_event(ctx, &core_command);
        self.handle_and_serialize(ctx, &event)
    }

    /// Retained read-only active-group highlight query over the existing
    /// `DescribePlan` transport. No mutation, no timer, no Meta state, no
    /// polling/retry/fallback: resolves the current retained focused leaf's
    /// immediate parent split-tree group in the requested focused domain,
    /// recursively includes its descendants, and projects them with the engine
    /// projector (never native/client rect topology). The carried
    /// `revision` is never a staleness gate (read-only snapshot resolves
    /// current retained state, so initial revision 0 and any lagging caller
    /// revision still resolve); the per-domain `base_revision` is returned
    /// as current snapshot metadata. Replies `active-group` with opaque
    /// group/member identities, the projected union bounds, and the
    /// owner/generation/correlation/base-revision identity; any
    /// invalid/missing/non-tiled focus, unknown domain/tree, or root-leaf
    /// focus replies `no-group`.
    ///
    /// Carried-window divergence safety (exact contract, no new topology
    /// authority): carried `windows` rectangles/sets are never consulted for
    /// membership or projection. Membership comes solely from the retained
    /// split tree via [`tiler_core::active_group::describe_active_group`],
    /// projection from retained bounds/gap plus the engine projector. The
    /// carried `focused_window` may update retained focus only through its
    /// guarded focus-sync path, while carried domain bounds/gap are bound to
    /// the retained domain. Drifted rects, extra/missing carried entries, or
    /// lagging revisions cannot corrupt topology or geometry: worst case is
    /// fail-closed `no-group` via `focus-unmapped`/`domain-mismatch`/pending/
    /// diverged.
    /// Direct-evaluator compatibility wrapper (test-only): exact legacy
    /// `from_value` + op-check behavior. Production `evaluate` bypasses this
    /// via the typed [`SyncCommand`] single parse + inner below.
    #[cfg(test)]
    fn evaluate_active_group_retained(&mut self, ctx: &Validated) -> String {
        let command: ActiveGroupCommand = match serde_json::from_value(ctx.request.command.clone())
        {
            Ok(command) => command,
            Err(error) => {
                let (kind, message) = classify_parse_error(&error);
                return rejected(valid_correlation_echo(&ctx.raw), kind, message);
            }
        };
        if command.op != "active-group" {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OPAQUE_ID,
                "active-group-op-invalid",
            );
        }
        self.evaluate_active_group_inner(ctx)
    }

    /// Direct-evaluator compatibility entry (test-only): shared focus-sync
    /// fences plus the typed resolution over a freshly built event.
    #[cfg(test)]
    fn evaluate_active_group_inner(&mut self, ctx: &Validated) -> String {
        let event = core_event(ctx, &tiler_core::boundary::CoreCommand::ActiveGroup);
        self.evaluate_active_group_with_event(ctx, &event)
    }

    /// Production typed active-group body: `core_command` is the total
    /// conversion of the already-decoded [`SyncCommand`] after all fences.
    /// Focus-sync plus read-only resolution run in
    /// `tiler_core::engine::Engine::handle`; serialization funnels through the
    /// typed choke point.
    fn evaluate_active_group_typed(
        &mut self,
        ctx: &Validated,
        core_command: &tiler_core::boundary::CoreCommand,
    ) -> String {
        debug_assert!(matches!(
            core_command,
            tiler_core::boundary::CoreCommand::ActiveGroup
        ));
        let event = core_event(ctx, core_command);
        self.evaluate_active_group_with_event(ctx, &event)
    }

    /// Shared active-group body over a total [`tiler_core::boundary::CoreEvent`]:
    /// Engine-owned focus-sync plus read-only resolution. Both production and
    /// test entries funnel here.
    fn evaluate_active_group_with_event(
        &mut self,
        ctx: &Validated,
        event: &tiler_core::boundary::CoreEvent,
    ) -> String {
        self.handle_and_serialize(ctx, event)
    }

    /// Shared standalone send target scope: optional `target_domain`
    /// plus `target_windows` against the source `domain`. Refuses
    /// same-workspace, and malformed target windows
    /// fail-closed with bounded kinds, and returns the projected target domain
    /// plus its key. Workspace send (`cross_output=false`) refuses
    /// cross-output targets; explicit output send (`cross_output=true`)
    /// requires a different output (same-output targets refuse) so the two
    /// ops stay distinct. No mover/command binding; the request and status paths
    /// add their own command checks.
    fn send_target_scope(
        &self,
        ctx: &Validated,
        cross_output: bool,
    ) -> Result<(OutputDomain, DomainKey), String> {
        let cid = ctx.request.correlation_id.clone();
        let Some(target_dto) = &ctx.request.target_domain else {
            return Err(rejected(
                cid,
                "workspace-target-invalid",
                "target workspace domain is missing",
            ));
        };
        if !is_opaque_id(&target_dto.output) {
            return Err(rejected(
                cid,
                "workspace-target-invalid",
                "target output is invalid",
            ));
        }
        if !is_opaque_id(&target_dto.workspace) {
            return Err(rejected(
                cid,
                "workspace-target-invalid",
                "target workspace is invalid",
            ));
        }
        if cross_output {
            if target_dto.output == ctx.request.domain.output {
                return Err(rejected(
                    cid,
                    "cross-domain-mismatch",
                    "target output equals the source output",
                ));
            }
        } else {
            if target_dto.output != ctx.request.domain.output {
                return Err(rejected(
                    cid,
                    "cross-output",
                    "target workspace is not on the focused output",
                ));
            }
            if target_dto.workspace == ctx.request.domain.workspace {
                return Err(rejected(
                    cid,
                    "unchanged-workspace",
                    "target workspace equals the source workspace",
                ));
            }
        }
        let carried_bounds = Rect {
            x: target_dto.bounds.x,
            y: target_dto.bounds.y,
            w: target_dto.bounds.w,
            h: target_dto.bounds.h,
        };
        if !valid_carried_rect(
            carried_bounds.x,
            carried_bounds.y,
            carried_bounds.w,
            carried_bounds.h,
        ) {
            return Err(snapshot_invalid(
                cid,
                MSG_OBSERVATION,
                "domain-bounds-invalid",
            ));
        }
        if target_dto.gap < 0 {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "gap-low"));
        }
        if target_dto.gap > GEOMETRY_MAX_GAP {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "gap-high"));
        }
        if target_dto.outer_gap < 0 {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "outer-gap-low"));
        }
        if target_dto.outer_gap > GEOMETRY_MAX_GAP {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "outer-gap-high"));
        }
        {
            let mut seen = std::collections::HashSet::new();
            for entry in &ctx.request.target_windows {
                if !is_opaque_id(&entry.window) {
                    return Err(snapshot_invalid(
                        cid,
                        MSG_OPAQUE_ID,
                        "observed-window-invalid",
                    ));
                }
                if !is_opaque_id(&entry.output) {
                    return Err(snapshot_invalid(
                        cid,
                        MSG_OPAQUE_ID,
                        "observed-output-invalid",
                    ));
                }
                if !is_opaque_id(&entry.workspace) {
                    return Err(snapshot_invalid(
                        cid,
                        MSG_OPAQUE_ID,
                        "observed-workspace-invalid",
                    ));
                }
                if entry.output != target_dto.output || entry.workspace != target_dto.workspace {
                    return Err(rejected(cid, "cross-domain-mismatch", MSG_CROSS_DOMAIN));
                }
                if !valid_carried_rect(entry.rect.x, entry.rect.y, entry.rect.w, entry.rect.h) {
                    return Err(snapshot_invalid(
                        cid,
                        MSG_OBSERVATION,
                        "window-rect-invalid",
                    ));
                }
                if !seen.insert(entry.window.clone()) {
                    return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "duplicate-window"));
                }
            }
        }
        let Ok(projected_target) =
            tiler_core::geometry::inset_bounds(carried_bounds, target_dto.outer_gap)
        else {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "inset-exhausted"));
        };
        let target_domain = OutputDomain {
            id: OutputId(target_dto.output.clone()),
            workspace: WorkspaceId(target_dto.workspace.clone()),
            bounds: projected_target,
            gap: target_dto.gap,
            adjacent: std::collections::BTreeMap::new(),
        };
        if !target_domain.validate() {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "domain-invalid"));
        }
        let target_key = DomainKey {
            output: OutputId(target_dto.output.clone()),
            workspace: WorkspaceId(target_dto.workspace.clone()),
        };
        Ok((target_domain, target_key))
    }

    /// Validate the standalone workspace-send target: optional `target_domain`
    /// plus `target_windows` against the source `domain`/`windows`. Refuses
    /// cross-output, same-workspace, absent/invalid focus, and malformed
    /// target windows fail-closed with bounded kinds.
    fn validate_workspace_input(&self, ctx: &Validated) -> Result<WorkspaceInput, String> {
        let cid = ctx.request.correlation_id.clone();
        // Target scope first (presence, cross-output, bounds, homing), then
        // the mover binding; a missing target still reports
        // `workspace-target-invalid` from the shared scope helper.
        let (target_domain, target_key) = self.send_target_scope(ctx, false)?;
        if ctx.request.focused_window.is_empty() {
            return Err(rejected(
                cid,
                "absent-focus",
                "no focused window is observed",
            ));
        }
        // Strict tagged decode after the target scope and focus checks above
        // (see `SyncCommand`): scope-before-parse order is unchanged. A
        // missing `follow` defaults true, preserving the historical follow
        // behavior for legacy requests.
        let (window, target_output, target_workspace, follow) =
            match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
                Ok(SyncCommand::SendToWorkspace {
                    window,
                    target_output,
                    target_workspace,
                    follow,
                }) => (window, target_output, target_workspace, follow),
                Ok(_) => {
                    return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-op-invalid"));
                }
                Err(error) => {
                    if is_unknown_variant(&error) {
                        return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-op-invalid"));
                    }
                    let (kind, message) = classify_parse_error(&error);
                    return Err(rejected(valid_correlation_echo(&ctx.raw), kind, message));
                }
            };
        if !is_opaque_id(&window) {
            return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-window-invalid"));
        }
        if !is_opaque_id(&target_output) {
            return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-output-invalid"));
        }
        if !is_opaque_id(&target_workspace) {
            return Err(snapshot_invalid(
                cid,
                MSG_OPAQUE_ID,
                "move-workspace-invalid",
            ));
        }
        if window != ctx.request.focused_window {
            return Err(rejected(
                cid,
                "focus-mismatch",
                "the moved window is not the focused window",
            ));
        }
        if target_output != target_key.output.0 || target_workspace != target_key.workspace.0 {
            return Err(rejected(
                cid,
                "target-mismatch",
                "command target does not match the target domain",
            ));
        }
        Ok(WorkspaceInput {
            target_domain,
            target_key,
            window: WindowId(window),
            follow,
        })
    }

    /// Workspace-send request phase: propose the same-output
    /// distinct-workspace move synchronously through the Engine with native
    /// assignment plus both-domain geometry.
    ///
    /// Codec/scope stays here (shared target scope, tagged command decode,
    /// mover binding in [`Planner::validate_workspace_input`]); the outcome
    /// itself is the Engine-owned [`tiler_core::engine::Engine::handle`]
    /// typed entry point over the validated target scope.
    fn evaluate_workspace_request(&mut self, ctx: &Validated) -> String {
        let input = match self.validate_workspace_input(ctx) {
            Ok(input) => input,
            Err(reply) => return reply,
        };
        let core_command = tiler_core::boundary::CoreCommand::SendToWorkspace {
            window: input.window.0.clone(),
            target_output: input.target_key.output.0.clone(),
            target_workspace: input.target_key.workspace.0.clone(),
            follow: input.follow,
        };
        let mut event = core_event(ctx, &core_command);
        event.target_domain = Some((input.target_domain, input.target_key));
        self.handle_and_serialize(ctx, &event)
    }

    /// Validate the explicit output-send target: shared send scope in
    /// cross-output mode (a different output's current workspace, resolved
    /// adapter-side) plus the mover binding. Same-output targets refuse;
    /// same-domain targets refuse downstream as `Unchanged`; unknown targets
    /// as `UnknownDomain`. Otherwise mirrors [`Planner::validate_workspace_input`].
    fn validate_send_output_input(&self, ctx: &Validated) -> Result<WorkspaceInput, String> {
        let cid = ctx.request.correlation_id.clone();
        let (target_domain, target_key) = self.send_target_scope(ctx, true)?;
        if ctx.request.focused_window.is_empty() {
            return Err(rejected(
                cid,
                "absent-focus",
                "no focused window is observed",
            ));
        }
        // Strict tagged decode after the target scope and focus checks above
        // (scope-before-parse order mirrors workspace send). A missing
        // `follow` defaults true, preserving the follow default for legacy
        // requests.
        let (window, target_output, target_workspace, follow) =
            match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
                Ok(SyncCommand::SendToOutput {
                    window,
                    target_output,
                    target_workspace,
                    follow,
                }) => (window, target_output, target_workspace, follow),
                Ok(_) => {
                    return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-op-invalid"));
                }
                Err(error) => {
                    if is_unknown_variant(&error) {
                        return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-op-invalid"));
                    }
                    let (kind, message) = classify_parse_error(&error);
                    return Err(rejected(valid_correlation_echo(&ctx.raw), kind, message));
                }
            };
        if !is_opaque_id(&window) {
            return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-window-invalid"));
        }
        if !is_opaque_id(&target_output) {
            return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "move-output-invalid"));
        }
        if !is_opaque_id(&target_workspace) {
            return Err(snapshot_invalid(
                cid,
                MSG_OPAQUE_ID,
                "move-workspace-invalid",
            ));
        }
        if window != ctx.request.focused_window {
            return Err(rejected(
                cid,
                "focus-mismatch",
                "the moved window is not the focused window",
            ));
        }
        if target_output != target_key.output.0 || target_workspace != target_key.workspace.0 {
            return Err(rejected(
                cid,
                "target-mismatch",
                "command target does not match the target domain",
            ));
        }
        Ok(WorkspaceInput {
            target_domain,
            target_key,
            window: WindowId(window),
            follow,
        })
    }

    /// Explicit output-send request phase: propose the cross-output move to
    /// the destination output's current workspace synchronously through the
    /// Engine with native assignment plus both-domain geometry.
    ///
    /// Codec/scope stays here (shared send scope in cross-output mode,
    /// tagged command decode, mover binding in
    /// [`Planner::validate_send_output_input`]); the outcome itself is the
    /// Engine-owned [`tiler_core::engine::Engine::handle`] typed entry point
    /// over the validated target scope.
    fn evaluate_send_output_request(&mut self, ctx: &Validated) -> String {
        let input = match self.validate_send_output_input(ctx) {
            Ok(input) => input,
            Err(reply) => return reply,
        };
        let core_command = tiler_core::boundary::CoreCommand::SendToOutput {
            window: input.window.0.clone(),
            target_output: input.target_key.output.0.clone(),
            target_workspace: input.target_key.workspace.0.clone(),
            follow: input.follow,
        };
        let mut event = core_event(ctx, &core_command);
        event.target_domain = Some((input.target_domain, input.target_key));
        self.handle_and_serialize(ctx, &event)
    }

    /// Whole-workspace migration target scope: the target keeps the source
    /// workspace id on a different output; `target_windows` must be empty
    /// (a DIFFERENT current workspace on the destination stays KDE-native).
    fn migrate_target_scope(&self, ctx: &Validated) -> Result<(OutputDomain, DomainKey), String> {
        let cid = ctx.request.correlation_id.clone();
        let Some(target_dto) = &ctx.request.target_domain else {
            return Err(rejected(
                cid,
                "workspace-target-invalid",
                "target workspace domain is missing",
            ));
        };
        if !is_opaque_id(&target_dto.output) {
            return Err(rejected(
                cid,
                "workspace-target-invalid",
                "target output is invalid",
            ));
        }
        if !is_opaque_id(&target_dto.workspace) {
            return Err(rejected(
                cid,
                "workspace-target-invalid",
                "target workspace is invalid",
            ));
        }
        if target_dto.output == ctx.request.domain.output {
            return Err(rejected(
                cid,
                "cross-domain-mismatch",
                "target output equals the source output",
            ));
        }
        if target_dto.workspace != ctx.request.domain.workspace {
            return Err(rejected(cid, "cross-domain-mismatch", MSG_CROSS_DOMAIN));
        }
        if !ctx.request.target_windows.is_empty() {
            return Err(rejected(
                cid,
                RefusalKind::Unchanged.as_str(),
                RefusalKind::Unchanged.message(),
            ));
        }
        let carried_bounds = Rect {
            x: target_dto.bounds.x,
            y: target_dto.bounds.y,
            w: target_dto.bounds.w,
            h: target_dto.bounds.h,
        };
        if !valid_carried_rect(
            carried_bounds.x,
            carried_bounds.y,
            carried_bounds.w,
            carried_bounds.h,
        ) {
            return Err(snapshot_invalid(
                cid,
                MSG_OBSERVATION,
                "domain-bounds-invalid",
            ));
        }
        if target_dto.gap < 0 {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "gap-low"));
        }
        if target_dto.gap > GEOMETRY_MAX_GAP {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "gap-high"));
        }
        if target_dto.outer_gap < 0 {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "outer-gap-low"));
        }
        if target_dto.outer_gap > GEOMETRY_MAX_GAP {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "outer-gap-high"));
        }
        let Ok(projected_target) =
            tiler_core::geometry::inset_bounds(carried_bounds, target_dto.outer_gap)
        else {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "inset-exhausted"));
        };
        let target_domain = OutputDomain {
            id: OutputId(target_dto.output.clone()),
            workspace: WorkspaceId(target_dto.workspace.clone()),
            bounds: projected_target,
            gap: target_dto.gap,
            adjacent: std::collections::BTreeMap::new(),
        };
        if !target_domain.validate() {
            return Err(snapshot_invalid(cid, MSG_OBSERVATION, "domain-invalid"));
        }
        let target_key = DomainKey {
            output: OutputId(target_dto.output.clone()),
            workspace: WorkspaceId(target_dto.workspace.clone()),
        };
        Ok((target_domain, target_key))
    }

    /// Validate the migration request: target scope plus the directional
    /// verb. No mover binding (whole domain), no follow flag (follow-only).
    /// Empty members require empty focus; the Engine owns the rest.
    fn validate_migrate_input(
        &self,
        ctx: &Validated,
    ) -> Result<(OutputDomain, DomainKey, String), String> {
        let cid = ctx.request.correlation_id.clone();
        let (target_domain, target_key) = self.migrate_target_scope(ctx)?;
        if ctx.request.windows.is_empty() && !ctx.request.focused_window.is_empty() {
            return Err(rejected(
                cid,
                RefusalKind::FocusMismatch.as_str(),
                RefusalKind::FocusMismatch.message(),
            ));
        }
        let direction = match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
            Ok(SyncCommand::MigrateWorkspace { direction }) => direction,
            Ok(_) => {
                return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "migrate-op-invalid"));
            }
            Err(error) => {
                if is_unknown_variant(&error) {
                    return Err(snapshot_invalid(cid, MSG_OPAQUE_ID, "migrate-op-invalid"));
                }
                let (kind, message) = classify_parse_error(&error);
                return Err(rejected(valid_correlation_echo(&ctx.raw), kind, message));
            }
        };
        if parse_direction(&direction).is_none() {
            return Err(rejected(cid, "direction-invalid", MSG_DIRECTION));
        }
        Ok((target_domain, target_key, direction))
    }

    /// Explicit whole-workspace migration request phase: retained rekey
    /// through the Engine. Reply `planned` means retained rekey only.
    fn evaluate_migrate_workspace_request(&mut self, ctx: &Validated) -> String {
        let (target_domain, target_key, direction) = match self.validate_migrate_input(ctx) {
            Ok(input) => input,
            Err(reply) => return reply,
        };
        let core_command = tiler_core::boundary::CoreCommand::MigrateWorkspace { direction };
        let mut event = core_event(ctx, &core_command);
        event.target_domain = Some((target_domain, target_key));
        self.handle_and_serialize(ctx, &event)
    }
}

fn evaluate_toggle_float_with(
    ctx: &Validated,
    evaluate: impl FnOnce(&str, Option<Rect>) -> String,
) -> String {
    // Strict tagged decode in place (see `SyncCommand`): runs after the
    // caller's raw-window floating probe, preserving `not-tiled` precedence
    // for malformed floats on untracked floating windows.
    let (window, float_rect_dto) =
        match serde_json::from_value::<SyncCommand>(ctx.request.command.clone()) {
            Ok(SyncCommand::ToggleFloat { window, float_rect }) => (window, float_rect),
            Ok(_) => {
                return snapshot_invalid(
                    ctx.request.correlation_id.clone(),
                    MSG_OBSERVATION,
                    "toggle-float-op-invalid",
                );
            }
            Err(error) => {
                if is_unknown_variant(&error) {
                    return snapshot_invalid(
                        ctx.request.correlation_id.clone(),
                        MSG_OBSERVATION,
                        "toggle-float-op-invalid",
                    );
                }
                let (kind, message) = classify_parse_error(&error);
                return rejected(valid_correlation_echo(&ctx.raw), kind, message);
            }
        };
    if !is_opaque_id(&window) {
        return snapshot_invalid(
            ctx.request.correlation_id.clone(),
            MSG_OBSERVATION,
            "toggle-float-window-invalid",
        );
    }
    if !ctx
        .request
        .windows
        .iter()
        .any(|entry| entry.window == window)
    {
        return rejected(
            ctx.request.correlation_id.clone(),
            "partial-observation",
            MSG_OBSERVATION,
        );
    }
    let float_rect = match float_rect_dto.as_ref() {
        Some(rect) if valid_carried_rect(rect.x, rect.y, rect.w, rect.h) => Some(Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }),
        Some(_) => {
            return snapshot_invalid(
                ctx.request.correlation_id.clone(),
                MSG_OBSERVATION,
                "float-rect-invalid",
            );
        }
        None => None,
    };
    evaluate(&window, float_rect)
}

/// Directional move/focus plumbing value for the canonical pair path.
/// Rebuilt by the converted `move`/`focus` handlers from the typed
/// [`SyncCommand`] variant (never parsed directly anymore); the
/// `cross_output_transfer` default-true is mirrored on the enum variants so
/// omitted legacy requests keep their historical full-capability behavior.
/// The `same_axis_move` default mirrors the R-MOV-03 missing-field rule so
/// omitted legacy requests keep the historical group-with-neighbor behavior.
#[derive(Debug, Clone)]
struct DirectedCommand {
    window: String,
    direction: String,
    /// Active KWin currently has no public output-transfer primitive. The
    /// adapter sends false for a two-domain flight, preserving local movement
    /// while rejecting R4 before the planner stages any state. Omitted legacy
    /// requests retain their historical full-capability behavior.
    cross_output_transfer: bool,
    /// Validated R-MOV-03 same-axis mode (missing wire field decodes to
    /// [`SameAxisMove::GroupWithNeighbor`]; anything else refuses as
    /// `move-op-invalid` before this value exists).
    same_axis_move: SameAxisMove,
    /// Float-origin subject marker for focus only (ignored on move): true
    /// means the subject is a floating/sticky exception and the Engine must
    /// use the shared cross-output remembered-target fallback without the
    /// tiled local-edge check. Move commands never set this.
    float_subject: bool,
}

const fn default_cross_output_transfer() -> bool {
    true
}

/// R-MOV-03 missing-field default: omitted `same_axis_move` decodes to
/// `group-with-neighbor`, preserving the historical wrap behavior byte-for-byte.
fn default_same_axis_move() -> String {
    SameAxisMove::GroupWithNeighbor.as_wire_str().to_owned()
}

/// R-SPC-04 D1 missing-field default: omitted `fixed_size_predicate` decodes
/// to `both-axes-fixed`, preserving the delivered both-axes behavior
/// byte-for-byte.
fn default_fixed_size_predicate() -> String {
    tiler_core::size_hints::FixedSizePredicate::BothAxes
        .as_wire_str()
        .to_owned()
}

/// Omitted `follow` on `send-to-workspace` preserves the historical follow
/// behavior (mirrors [`default_cross_output_transfer`]).
const fn default_follow() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(test)]
struct ReconcileCommand {
    op: String,
}

/// Read-only active-group highlight query: no parameters beyond the shared
/// observation envelope (domain/windows/focus) plus identity. Strict shape:
/// extra fields reject via the established unknown-field path.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(test)]
struct ActiveGroupCommand {
    op: String,
}

/// Wire stored next hover for the carried sticky group-edge prior: the group
/// id plus the edge direction (`left`/`right`/`top`/`bottom`; `center` never
/// carries stickiness). Strict shape via `deny_unknown_fields`; semantic
/// validity (group id, edge token) is judged at conversion, where invalid
/// values degrade to ignoring the whole carry (never refused, never enabling
/// arbitrary hover).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DragPriorDto {
    group: String,
    edge: String,
}

/// Wire sticky hover carry for `drag-drop`/`drag-preview`: the exact drag
/// source identity plus the capture revision and the stored next hover
/// (`prior` null/absent when the last hover was not a group edge). Strict
/// shape via `deny_unknown_fields`; semantic validity (opaque ids, revision
/// bound, prior edge) is judged at conversion, where any invalid value
/// degrades to ignoring the whole carry advisory-style.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DragHoverPriorDto {
    domain_output: String,
    domain_workspace: String,
    source_leaf: String,
    source_window: String,
    revision: u64,
    #[serde(default)]
    prior: Option<DragPriorDto>,
}

/// Convert a decoded hover carry into the portable core value. Returns `None`
/// when the carry is absent or semantically invalid (non-opaque ids, revision
/// over the bound, non-edge or unknown prior edge/group): the caller then
/// proceeds without a carry so an invalid prior can neither enable arbitrary
/// hover nor clear valid sticky state. Wire-shape violations never reach here
/// (they already refused via the tagged decode).
fn convert_hover_prior(
    dto: Option<&DragHoverPriorDto>,
) -> Option<tiler_core::session::DragHoverPrior> {
    let dto = dto.as_ref()?;
    if !is_opaque_id(&dto.domain_output)
        || !is_opaque_id(&dto.domain_workspace)
        || !is_opaque_id(&dto.source_leaf)
        || !is_opaque_id(&dto.source_window)
        || dto.revision > PLAN_MAX_REVISION
    {
        return None;
    }
    let prior = match &dto.prior {
        None => None,
        Some(prior) => {
            if !is_opaque_id(&prior.group) {
                return None;
            }
            let edge = match prior.edge.as_str() {
                "left" => tiler_core::contract::DragSide::Left,
                "right" => tiler_core::contract::DragSide::Right,
                "top" => tiler_core::contract::DragSide::Top,
                "bottom" => tiler_core::contract::DragSide::Bottom,
                _ => return None,
            };
            Some(tiler_core::policy::PriorGroupEdge {
                group: NodeId::from(prior.group.as_str()),
                edge,
            })
        }
    };
    Some(tiler_core::session::DragHoverPrior {
        domain: DomainKey {
            output: OutputId(dto.domain_output.clone()),
            workspace: WorkspaceId(dto.domain_workspace.clone()),
        },
        source_leaf: NodeId::from(dto.source_leaf.as_str()),
        source_window: WindowId(dto.source_window.clone()),
        revision: dto.revision,
        prior,
        // Wire carries pre-pair single-domain hovers only: the resolution
        // scope is the capture domain itself. (Cross-output pair previews
        // never cross this boundary; their scope rides the Engine value.)
        prior_domain: DomainKey {
            output: OutputId(dto.domain_output.clone()),
            workspace: WorkspaceId(dto.domain_workspace.clone()),
        },
    })
}

/// Shared drag [`tiler_core::boundary::CoreCommand`] construction over one
/// [`DragPayload`]: advisory hover carry plus Started source binding plus the
/// pointer. Single source so drop and preview cannot drift; `preview` selects
/// the read-only variant. Invalid carry/binding degrades to no carry /
/// same-output (never refused, never enabling arbitrary hover).
fn drag_core_command(payload: &DragPayload, preview: bool) -> tiler_core::boundary::CoreCommand {
    use tiler_core::boundary::CoreCommand;
    let source = match (&payload.source_output, &payload.source_workspace) {
        (Some(output), Some(workspace)) if is_opaque_id(output) && is_opaque_id(workspace) => {
            Some(DomainKey {
                output: OutputId(output.clone()),
                workspace: WorkspaceId(workspace.clone()),
            })
        }
        _ => None,
    };
    let hover_prior = convert_hover_prior(payload.hover_prior.as_ref());
    if preview {
        CoreCommand::DragPreview {
            window: payload.window.clone(),
            x: payload.x,
            y: payload.y,
            hover_prior,
            source,
        }
    } else {
        CoreCommand::DragDrop {
            window: payload.window.clone(),
            x: payload.x,
            y: payload.y,
            hover_prior,
            source,
        }
    }
}

/// Shared `drag-drop`/`drag-preview` request shape (option B): pointer `x`/`y`,
/// an advisory sticky hover carry, and an advisory Started cross-output source
/// binding. Single source so both `SyncCommand` variants stay wire-identical;
/// distinct `op` tokens and the read-only preview reply stay distinct. Unknown
/// fields still refuse via `deny_unknown_fields`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DragPayload {
    window: String,
    x: i32,
    y: i32,
    /// Optional carried sticky hover prior, applied advisory-style between
    /// begin and drop/preview. Absent preserves legacy behavior.
    #[serde(default)]
    hover_prior: Option<DragHoverPriorDto>,
    /// Optional Started cross-output source binding (source output). Absent
    /// preserves same-output behavior; paired with `source_workspace`,
    /// validated opaque, cross-output only.
    #[serde(default)]
    source_output: Option<String>,
    /// Optional Started cross-output source binding (source workspace).
    #[serde(default)]
    source_workspace: Option<String>,
}

/// Typed synchronous command codec (narrow).
///
/// Internally tagged on `op` with `deny_unknown_fields` for all fifteen
/// synchronous command ops: reconcile, update-gaps, active-group,
/// release-domain, move, focus, resize, pointer-resize, toggle-float,
/// toggle-orientation, `send-to-workspace`, `send-to-output`,
/// `migrate-workspace`, `drag-drop`, and read-only `drag-preview`.
/// Sync handlers parse
/// [`SyncCommand`] once in place after the existing dispatch boundaries
/// (validation, send dispatch, binding sync): the production `evaluate`
/// string-guards on the known op before dispatch, so missing/non-string/
/// unknown ops keep the exact `unknown-value` path without a typed parse.
/// A present-but-wrong op string surfaces as an
/// `unknown variant` decode error, which each handler maps back to the exact
/// legacy `*-op-invalid` snapshot the old `from_value` + op-check produced;
/// missing/non-string/extra-field errors keep `classify_parse_error`
/// behavior. Direct `evaluate_*` tests therefore pass unchanged.
/// `move`/`focus` rebuild the [`DirectedCommand`] plumbing value from the
/// typed variant for the untouched directional pair path (including the
/// `cross_output_transfer` default-true); `toggle-float` keeps its raw-window
/// floating probe before the typed parse, so malformed floats on untracked
/// floating windows still report `not-tiled`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum SyncCommand {
    #[serde(rename = "reconcile")]
    Reconcile {},
    #[serde(rename = "update-gaps")]
    UpdateGaps {},
    #[serde(rename = "active-group")]
    ActiveGroup {},
    #[serde(rename = "release-domain")]
    ReleaseDomain {},
    #[serde(rename = "move")]
    Move {
        window: String,
        direction: String,
        #[serde(default = "default_cross_output_transfer")]
        cross_output_transfer: bool,
        /// R-MOV-03 same-axis mode (`group-with-neighbor` /
        /// `swap-with-neighbor`). Absent preserves the historical wrap
        /// behavior; unknown values refuse as `move-op-invalid` at the
        /// handler (never silently coerced).
        #[serde(default = "default_same_axis_move")]
        same_axis_move: String,
    },
    #[serde(rename = "focus")]
    Focus {
        window: String,
        direction: String,
        #[serde(default = "default_cross_output_transfer")]
        cross_output_transfer: bool,
        /// Float-origin cross-output focus (adapter local float search missed
        /// Left/Right). Absent/false preserves every existing tile-origin
        /// request byte-for-byte; true requires a two-domain Left/Right
        /// request and fails closed otherwise. Non-boolean values refuse via
        /// the established parse-error path.
        #[serde(default)]
        float_subject: bool,
    },
    #[serde(rename = "resize")]
    Resize {
        window: String,
        direction: String,
        mode: String,
        press_index: u32,
    },
    #[serde(rename = "pointer-resize")]
    PointerResize {
        window: String,
        direction: String,
        boundary: i32,
        /// Corner second axis: both present for an atomic corner resize,
        /// both absent for an ordinary single-axis request. `deny_unknown_fields`
        /// is preserved; single-axis JSON is shape-identical to before.
        #[serde(default)]
        direction2: Option<String>,
        #[serde(default)]
        boundary2: Option<i32>,
    },
    #[serde(rename = "toggle-float")]
    ToggleFloat {
        window: String,
        #[serde(default)]
        float_rect: Option<RectDto>,
    },
    #[serde(rename = "toggle-orientation")]
    ToggleOrientation { window: String },
    #[serde(rename = "send-to-workspace")]
    SendToWorkspace {
        window: String,
        target_output: String,
        target_workspace: String,
        /// Follow/stay selection: true follows the mover into the target,
        /// false stays on the source. Absent preserves the historical
        /// follow behavior.
        #[serde(default = "default_follow")]
        follow: bool,
    },
    /// Explicit output send (REQ-OUT-04, item 5.3/5.4): the destination
    /// output's current workspace resolved adapter-side, carried as an
    /// explicit cross-output target domain. Distinct wire op from
    /// `send-to-workspace`, which stays same-output. Ordinary admission and
    /// explicit follow/stay; omitted `follow` defaults true like workspace
    /// send. Follow binds Meta/Win+Ctrl+Alt+arrows and +H/J/K/L; stay is
    /// bindable, unbound (adapter-owned bindings).
    #[serde(rename = "send-to-output")]
    SendToOutput {
        window: String,
        target_output: String,
        target_workspace: String,
        /// Follow/stay selection, identical to `send-to-workspace`.
        #[serde(default = "default_follow")]
        follow: bool,
    },
    /// Explicit whole-workspace output migration (R-WS-12): the active
    /// workspace keeps its stable backing id and moves to the adjacent
    /// output resolved adapter-side via full-output-rect adjacency (no wrap).
    /// Follow-only (no `follow` field); the adapter shows the migrated
    /// workspace on the target and refills the source natively. Four
    /// directional actions, bindable and unbound by default (adapter-owned
    /// bindings). Strict shape via `deny_unknown_fields` on the enum.
    #[serde(rename = "migrate-workspace")]
    MigrateWorkspace { direction: String },
    #[serde(rename = "drag-drop")]
    DragDrop(DragPayload),
    #[serde(rename = "drag-preview")]
    DragPreview(DragPayload),
}

/// Legacy op-mismatch mapping for converted handlers (see [`SyncCommand`]):
/// a tagged-decode `unknown variant` error means the carried op was present
/// but wrong, which the legacy struct parse + op-check reported as the
/// handler's `*-op-invalid` snapshot. All other decode errors keep
/// [`classify_parse_error`] behavior.
fn is_unknown_variant(error: &serde_json::Error) -> bool {
    error.to_string().contains("unknown variant")
}

/// Typed core boundary conversion.
///
/// Maps the already-decoded [`SyncCommand`] into
/// [`tiler_core::boundary::CoreCommand`] after the existing validation,
/// send-dispatch, and binding-sync boundaries. Total for the synchronous
/// routes: clones already-validated values, never validates, never re-parses.
/// Fallible wire vocabularies (direction/mode) cross opaquely so
/// handler-local precedence (`not-tiled`, `*-op-invalid`) is untouched.
/// Directional pair state comes from the validated
/// `directional_domains`/`directional_keys`; the workspace-send target stays
/// route-local (validated `WorkspaceInput`).
fn core_command_from_sync(command: &SyncCommand) -> Option<tiler_core::boundary::CoreCommand> {
    use tiler_core::boundary::CoreCommand;
    match command {
        SyncCommand::Reconcile {} => Some(CoreCommand::Reconcile),
        SyncCommand::UpdateGaps {} => Some(CoreCommand::UpdateGaps),
        SyncCommand::ActiveGroup {} => Some(CoreCommand::ActiveGroup),
        SyncCommand::ReleaseDomain {} => Some(CoreCommand::ReleaseDomain),
        SyncCommand::Move {
            window,
            direction,
            cross_output_transfer,
            same_axis_move,
        } => Some(CoreCommand::Move {
            window: window.clone(),
            direction: direction.clone(),
            cross_output_transfer: *cross_output_transfer,
            // Validated at the move handlers before conversion; `None` here
            // is unreachable and fails closed at the callers' `expect`.
            same_axis_move: SameAxisMove::parse_wire(same_axis_move)?,
        }),
        SyncCommand::Focus {
            window,
            direction,
            cross_output_transfer,
            float_subject,
        } => Some(CoreCommand::Focus {
            window: window.clone(),
            direction: direction.clone(),
            cross_output_transfer: *cross_output_transfer,
            float_subject: *float_subject,
        }),
        SyncCommand::Resize {
            window,
            direction,
            mode,
            press_index,
        } => Some(CoreCommand::Resize {
            window: window.clone(),
            direction: direction.clone(),
            mode: mode.clone(),
            press_index: *press_index,
        }),
        SyncCommand::PointerResize {
            window,
            direction,
            boundary,
            direction2,
            boundary2,
        } => Some(CoreCommand::PointerResize {
            window: window.clone(),
            direction: direction.clone(),
            boundary: *boundary,
            direction2: direction2.clone(),
            boundary2: *boundary2,
        }),
        SyncCommand::ToggleFloat { window, float_rect } => Some(CoreCommand::ToggleFloat {
            window: window.clone(),
            float_rect: float_rect.as_ref().map(|rect| Rect {
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
            }),
        }),
        SyncCommand::ToggleOrientation { window } => Some(CoreCommand::ToggleOrientation {
            window: window.clone(),
        }),
        SyncCommand::SendToWorkspace {
            window,
            target_output,
            target_workspace,
            follow,
        } => Some(CoreCommand::SendToWorkspace {
            window: window.clone(),
            target_output: target_output.clone(),
            target_workspace: target_workspace.clone(),
            follow: *follow,
        }),
        SyncCommand::SendToOutput {
            window,
            target_output,
            target_workspace,
            follow,
        } => Some(CoreCommand::SendToOutput {
            window: window.clone(),
            target_output: target_output.clone(),
            target_workspace: target_workspace.clone(),
            follow: *follow,
        }),
        SyncCommand::MigrateWorkspace { direction } => Some(CoreCommand::MigrateWorkspace {
            direction: direction.clone(),
        }),
        SyncCommand::DragDrop(payload) => Some(drag_core_command(payload, false)),
        SyncCommand::DragPreview(payload) => Some(drag_core_command(payload, true)),
    }
}

/// Total [`tiler_core::boundary::CoreEvent`] construction from validated
/// state plus an already-decoded command. See
/// [`core_command_from_sync`] for the precedence contract.
fn core_event(
    ctx: &Validated,
    command: &tiler_core::boundary::CoreCommand,
) -> tiler_core::boundary::CoreEvent {
    let directional = match (&ctx.directional_domains, &ctx.directional_keys) {
        (Some(domains), Some(keys)) => {
            Some(domains.iter().cloned().zip(keys.iter().cloned()).collect())
        }
        _ => None,
    };
    let directional_target_outer_gap = ctx
        .raw
        .get("domains")
        .and_then(serde_json::Value::as_array)
        .and_then(|entries| entries.get(1))
        .and_then(|entry| entry.get("outer_gap"))
        .and_then(serde_json::Value::as_i64)
        .filter(|gap| (0..=i64::from(GEOMETRY_MAX_GAP)).contains(gap))
        .map(|gap| gap as i32);
    tiler_core::boundary::CoreEvent {
        owner: ctx.owner.clone(),
        generation: ctx.generation.clone(),
        correlation: ctx.correlation.clone(),
        revision: ctx.request.revision,
        fingerprint: ctx.request.fingerprint,
        domain: ctx.domain.clone(),
        domain_key: ctx.domain_key.clone(),
        outer_gap: ctx.request.domain.outer_gap,
        focused_window: WindowId(ctx.request.focused_window.clone()),
        windows: ctx
            .request
            .windows
            .iter()
            .map(engine_window_from_dto)
            .collect(),
        directional,
        directional_target_outer_gap,
        target_domain: None,
        target_windows: ctx
            .request
            .target_windows
            .iter()
            .map(engine_window_from_dto)
            .collect(),
        closed_windows: ctx
            .request
            .closed_windows
            .iter()
            .map(|id| WindowId(id.clone()))
            .collect(),
        command: command.clone(),
    }
}

/// Fixed `no-group` reasons (short lowercase-hyphenated ASCII, never echoes
/// input). Covers invalid/missing/non-tiled focus, unknown domain/tree,
/// root-leaf focus, projection failure, and pending/divergence.
/// `stale-revision` is retained in this closed registry for contract
/// compatibility but is no longer emitted: the read-only resolver returns the
/// current retained snapshot regardless of the carried revision.
const ACTIVE_GROUP_NO_GROUP_REASONS: &[&str] = &[
    "no-session",
    "diverged",
    "pending",
    "domain-mismatch",
    "stale-revision",
    "focus-mismatch",
    "focus-unmapped",
    "no-tree",
    "no-parent-group",
];

fn no_group_reply(ctx: &Validated, base_revision: Option<u64>, reason: &'static str) -> String {
    debug_assert!(ACTIVE_GROUP_NO_GROUP_REASONS.contains(&reason));
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: ctx.request.correlation_id.clone(),
        outcome: "no-group",
        kind: Some("no-group".to_owned()),
        message: None,
        base_revision,
        detail: Some(serde_json::json!({
            "kind": "no-group",
            "reason": reason,
            "owner": ctx.owner.as_str(),
            "generation": ctx.generation.as_str(),
        })),
        desired_geometry: None,
        desired_focus: None,
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

/// Byte-exact `active-group` found serializer for the [`serialize_core_reply`]
/// choke point (single source).
fn serialize_active_group_found(
    ctx: &Validated,
    found: &tiler_core::boundary::ActiveGroupFound,
) -> String {
    let members: Vec<serde_json::Value> = found
        .members
        .iter()
        .map(|member| {
            serde_json::json!({
                "window": member.window.0,
                "leaf": member.leaf.0,
                "rect": {"x": member.rect.x, "y": member.rect.y, "w": member.rect.w, "h": member.rect.h},
            })
        })
        .collect();
    let geometry: Vec<GeometryReply> = found
        .members
        .iter()
        .map(|member| GeometryReply {
            window: member.window.0.clone(),
            leaf: member.leaf.0.clone(),
            output: ctx.domain_key.output.0.clone(),
            workspace: ctx.domain_key.workspace.0.clone(),
            rect: RectDto {
                x: member.rect.x,
                y: member.rect.y,
                w: member.rect.w,
                h: member.rect.h,
            },
            // Read-only group query: no hint/clamp context applies.
            overconstrained: None,
            client_clamped: None,
        })
        .collect();
    serialize_bounded(&PlanReply {
        v: PLAN_CONTRACT_VERSION,
        correlation_id: ctx.request.correlation_id.clone(),
        outcome: "active-group",
        kind: Some("active-group".to_owned()),
        message: None,
        base_revision: Some(found.base_revision),
        detail: Some(serde_json::json!({
            "kind": "active-group",
            "owner": ctx.owner.as_str(),
            "generation": ctx.generation.as_str(),
            "domain_output": ctx.domain_key.output.0,
            "domain_workspace": ctx.domain_key.workspace.0,
            "group": found.group.0,
            "focused_leaf": found.focused_leaf.0,
            "focused_window": found.focused_window.0,
            "members": members,
            "bounds": {"x": found.bounds.x, "y": found.bounds.y, "w": found.bounds.w, "h": found.bounds.h},
        })),
        desired_geometry: Some(geometry),
        desired_focus: Some(focus_reply(&ctx.domain_key, &found.focused_leaf)),
        float_geometry: None,
        preconditions: None,
        operation: None,
        preview_rect: None,
        hover_prior: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    type SnapshotMutator = fn(&mut serde_json::Value);

    fn domain_bounds() -> serde_json::Value {
        serde_json::json!({"x": 0, "y": 0, "w": 1200, "h": 800})
    }

    fn window_entry(window: &str) -> serde_json::Value {
        let index = window
            .rsplit_once('-')
            .and_then(|(_, value)| value.parse::<i32>().ok())
            .unwrap_or(1);
        serde_json::json!({
            "window": window,
            "output": "out-1",
            "workspace": "ws-1",
            "rect": {"x": (index - 1) * 100, "y": 0, "w": 100, "h": 80},
        })
    }

    fn plan_request(
        correlation: &str,
        focused: &str,
        windows: &[&str],
        command: serde_json::Value,
    ) -> String {
        let entries: Vec<serde_json::Value> = windows.iter().map(|w| window_entry(w)).collect();
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": "owner-1",
            "generation": "gen-1",
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": domain_bounds(),
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": entries,
            "command": command,
        })
        .to_string()
    }

    fn parse_reply(reply: &str) -> serde_json::Value {
        serde_json::from_str(reply).expect("reply is JSON")
    }

    #[test]
    fn retained_toggle_float_carries_geometry_then_freshly_admits() {
        let mut planner = Planner::new();
        let float = plan_request(
            "float-1",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({
                "op": "toggle-float",
                "window": "win-1",
                "float_rect": {"x": 240, "y": 160, "w": 720, "h": 480},
            }),
        );
        let floated = parse_reply(&planner.evaluate(&float));
        assert_eq!(floated["outcome"], "planned");
        assert_eq!(
            floated["desired_geometry"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(floated["float_geometry"]["window"], "win-1");
        assert_eq!(floated["float_geometry"]["rect"]["w"], 720);

        let mut unfloat: serde_json::Value = serde_json::from_str(&plan_request(
            "float-2",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
        ))
        .expect("request JSON");
        unfloat["windows"][0]["floating"] = serde_json::Value::Bool(true);
        let tiled = parse_reply(&planner.evaluate(&unfloat.to_string()));
        assert_eq!(tiled["outcome"], "planned");
        assert_eq!(tiled["float_geometry"], serde_json::Value::Null);
        assert_eq!(tiled["desired_geometry"].as_array().map(Vec::len), Some(2));

        // Re-float selects the durable retained placement: no rect is carried
        // in the request, and the reply echoes the retained rectangle rather
        // than a recomputed center.
        let refloat = plan_request(
            "float-3",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
        );
        let refloated = parse_reply(&planner.evaluate(&refloat));
        assert_eq!(refloated["outcome"], "planned");
        assert_eq!(refloated["float_geometry"]["window"], "win-1");
        assert_eq!(
            refloated["float_geometry"]["rect"],
            serde_json::json!({"x": 240, "y": 160, "w": 720, "h": 480})
        );
    }

    #[test]
    fn retained_toggle_float_tracks_moved_live_geometry_across_cycle() {
        let mut planner = Planner::new();
        let float = plan_request(
            "moved-1",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
        );
        let floated = parse_reply(&planner.evaluate(&float));
        assert_eq!(floated["outcome"], "planned");
        // No explicit rect: the session resolves the centered 60% placement.
        assert_eq!(
            floated["float_geometry"]["rect"],
            serde_json::json!({"x": 240, "y": 160, "w": 720, "h": 480})
        );

        // Unfloat carries the user's moved live rectangle.
        let mut unfloat: serde_json::Value = serde_json::from_str(&plan_request(
            "moved-2",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({
                "op": "toggle-float",
                "window": "win-1",
                "float_rect": {"x": 300, "y": 200, "w": 500, "h": 400},
            }),
        ))
        .expect("request JSON");
        unfloat["windows"][0]["floating"] = serde_json::Value::Bool(true);
        let tiled = parse_reply(&planner.evaluate(&unfloat.to_string()));
        assert_eq!(tiled["outcome"], "planned");
        assert_eq!(tiled["float_geometry"], serde_json::Value::Null);

        // Re-float selects the moved retained rectangle, not the center.
        let refloat = plan_request(
            "moved-3",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
        );
        let refloated = parse_reply(&planner.evaluate(&refloat));
        assert_eq!(refloated["outcome"], "planned");
        assert_eq!(
            refloated["float_geometry"]["rect"],
            serde_json::json!({"x": 300, "y": 200, "w": 500, "h": 400})
        );
    }

    #[test]
    fn toggle_orientation_flips_once_and_restores_on_second() {
        let mut planner = Planner::new();
        let seed = plan_request(
            "orient-0",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "reconcile"}),
        );
        let seeded = parse_reply(&planner.evaluate(&seed));
        assert_eq!(seeded["outcome"], "planned");

        let first = plan_request(
            "orient-1",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-orientation", "window": "win-1"}),
        );
        let flipped = parse_reply(&planner.evaluate(&first));
        assert_eq!(flipped["outcome"], "planned");
        assert_eq!(flipped["detail"]["kind"], "toggle-orientation");
        assert_eq!(flipped["detail"]["capability"], "toggle-orientation");
        assert_eq!(flipped["detail"]["policy_version"], 1);
        assert_eq!(
            flipped["desired_geometry"].as_array().map(Vec::len),
            Some(2)
        );
        assert_ne!(
            flipped["desired_geometry"], seeded["desired_geometry"],
            "one toggle restacks the allocation"
        );

        let second = plan_request(
            "orient-2",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-orientation", "window": "win-1"}),
        );
        let restored = parse_reply(&planner.evaluate(&second));
        assert_eq!(restored["outcome"], "planned");
        assert_eq!(
            restored["desired_geometry"], seeded["desired_geometry"],
            "twice restores the seeded allocation"
        );
    }

    #[test]
    fn toggle_orientation_decode_is_strict() {
        let mut planner = Planner::new();
        // Missing window: malformed, never a plan.
        let missing = plan_request(
            "orient-bad-1",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-orientation"}),
        );
        let reply = parse_reply(&planner.evaluate(&missing));
        assert_eq!(reply["outcome"], "rejected");
        assert_eq!(reply["kind"], "request-malformed");
        // Unknown fields refuse fail-closed.
        let extra = plan_request(
            "orient-bad-2",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-orientation", "window": "win-1", "bogus": 1}),
        );
        let reply = parse_reply(&planner.evaluate(&extra));
        assert_eq!(reply["outcome"], "rejected");
        assert_eq!(reply["kind"], "unknown-field");
        // Empty window id is never admitted to the Engine.
        let empty = plan_request(
            "orient-bad-3",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-orientation", "window": ""}),
        );
        let reply = parse_reply(&planner.evaluate(&empty));
        assert_eq!(reply["outcome"], "rejected");
        assert_eq!(reply["kind"], "snapshot-invalid");
        assert_eq!(reply["detail"], "toggle-orient-window-invalid");
    }

    #[test]
    fn toggle_orientation_lone_leaf_and_floating_refuse() {
        // Lone root leaf: no parent group, so the toggle is a no-op.
        let mut lone = Planner::new();
        let seed = plan_request(
            "orient-lone-0",
            "win-1",
            &["win-1"],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&lone.evaluate(&seed))["outcome"], "planned");
        let toggle = plan_request(
            "orient-lone-1",
            "win-1",
            &["win-1"],
            serde_json::json!({"op": "toggle-orientation", "window": "win-1"}),
        );
        let reply = parse_reply(&lone.evaluate(&toggle));
        assert_eq!(reply["outcome"], "rejected");
        assert_eq!(reply["kind"], "unchanged");

        // Floating subject: the floated window is an exception, not tiled.
        let mut floated = Planner::new();
        let float = plan_request(
            "orient-float-0",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
        );
        assert_eq!(parse_reply(&floated.evaluate(&float))["outcome"], "planned");
        let mut request: serde_json::Value = serde_json::from_str(&plan_request(
            "orient-float-1",
            "win-2",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-orientation", "window": "win-1"}),
        ))
        .expect("request JSON");
        request["windows"][0]["floating"] = serde_json::Value::Bool(true);
        let reply = parse_reply(&floated.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected");
        assert_eq!(reply["kind"], "not-tiled");
    }

    #[test]
    fn toggle_orientation_wire_token_roundtrips_through_sync_command() {
        // The op token comes from the actual typed command, never a string
        // literal: encode op + window as JSON, decode via SyncCommand, and
        // assert the typed conversion equals the original command. No
        // production Serialize API exists for commands; this stays test-only.
        use tiler_core::boundary::CoreCommand;
        let typed = CoreCommand::ToggleOrientation {
            window: "win-1".to_owned(),
        };
        assert_eq!(typed.op(), "toggle-orientation");
        let wire = serde_json::json!({"op": typed.op(), "window": "win-1"});
        let decoded: SyncCommand = serde_json::from_value(wire).expect("decodes");
        assert_eq!(core_command_from_sync(&decoded), Some(typed));
    }

    #[test]
    fn toggle_orientation_plans_with_sibling_overlay_present() {
        // Sibling fullscreen/maximized overlays ride the wire as ordinary
        // tiles with fit_excluded (plan-adapter carriedSnapshot); only the
        // focused overlay refuses, in the adapter before dispatch. A
        // retained session therefore plans the toggle over the full set.
        let mut planner = Planner::new();
        let seed = plan_request(
            "orient-sib-0",
            "win-1",
            &["win-1", "win-2", "win-3"],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&seed))["outcome"], "planned");
        let mut request: serde_json::Value = serde_json::from_str(&plan_request(
            "orient-sib-1",
            "win-1",
            &["win-1", "win-2", "win-3"],
            serde_json::json!({"op": "toggle-orientation", "window": "win-1"}),
        ))
        .expect("request JSON");
        request["windows"][2]["fit_excluded"] = serde_json::Value::Bool(true);
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "planned");
        assert_eq!(reply["detail"]["kind"], "toggle-orientation");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(3),
            "sibling overlay keeps its planned geometry"
        );
    }

    #[test]
    fn retained_float_reconciles_then_moved_reconcile_then_unfloat() {
        // Exact-float incident: p5 float removes the tile and retains
        // placement, later reconciles converge newcomers around the
        // exception, a native float move arrives as reconcile, unfloat
        // returns the exception to tiled, and a later post-removal
        // reconcile still plans. Membership stays canonical and
        // incomplete observations still refuse fail-closed.
        fn float_request(
            correlation: &str,
            focused: &str,
            windows: &[(&str, i32, i32, i32, i32, bool)],
            command: serde_json::Value,
        ) -> String {
            let entries: Vec<serde_json::Value> = windows
                .iter()
                .map(|(window, x, y, w, h, floating)| {
                    let mut entry = serde_json::json!({
                        "window": window,
                        "output": "out-1",
                        "workspace": "ws-1",
                        "rect": {"x": x, "y": y, "w": w, "h": h},
                    });
                    if *floating {
                        entry["floating"] = serde_json::Value::Bool(true);
                        entry["fit_excluded"] = serde_json::Value::Bool(true);
                    }
                    entry
                })
                .collect();
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": 7,
                "domain": {
                    "output": "out-1",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                    "gap": 0,
                    "outer_gap": 0,
                },
                "focused_window": focused,
                "windows": entries,
                "command": command,
            })
            .to_string()
        }

        let mut planner = Planner::new();
        // Two tiled reconciles seed the domain.
        for (correlation, focused, windows, command) in [
            (
                "float-seq-1",
                "win-1",
                vec![("win-1", 0, 0, 100, 80, false)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "float-seq-2",
                "win-2",
                vec![
                    ("win-1", 0, 0, 100, 80, false),
                    ("win-2", 200, 0, 100, 80, false),
                ],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let reply = parse_reply(&planner.evaluate(&float_request(
                correlation,
                focused,
                &windows,
                command,
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        // Float removes the tile, retains placement, survivors stay tiled.
        let floated = parse_reply(&planner.evaluate(&float_request(
            "float-seq-3",
            "win-2",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 200, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "toggle-float", "window": "win-2"}),
        )));
        assert_eq!(floated["outcome"], "planned", "{floated}");
        assert_geometry_covers(&floated, &["win-1"]);
        assert_eq!(floated["float_geometry"]["window"], "win-2");
        // Later reconciles converge newcomers around the retained exception.
        let admitted3 = parse_reply(&planner.evaluate(&float_request(
            "float-seq-4",
            "win-3",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 240, 160, 720, 480, true),
                ("win-3", 400, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(admitted3["outcome"], "planned", "{admitted3}");
        assert_geometry_covers(&admitted3, &["win-1", "win-3"]);
        let admitted4 = parse_reply(&planner.evaluate(&float_request(
            "float-seq-5",
            "win-4",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 240, 160, 720, 480, true),
                ("win-3", 400, 0, 100, 80, false),
                ("win-4", 600, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(admitted4["outcome"], "planned", "{admitted4}");
        assert_geometry_covers(&admitted4, &["win-1", "win-3", "win-4"]);
        let before = geometry_by_window(&admitted4);
        // Native float movement arrives as reconcile with a drifted float rect.
        // The retained tiled allocation must project unchanged, not reject as
        // malformed-topology merely because an exception is present.
        let reconciled = parse_reply(&planner.evaluate(&float_request(
            "float-seq-6",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 100, 300, 720, 480, true),
                ("win-3", 400, 0, 100, 80, false),
                ("win-4", 600, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reconciled["outcome"], "planned", "{reconciled}");
        assert_eq!(reconciled["detail"]["kind"], "reconcile", "{reconciled}");
        assert_geometry_covers(&reconciled, &["win-1", "win-3", "win-4"]);
        assert_eq!(
            geometry_by_window(&reconciled),
            before,
            "{reconciled} vs {admitted4}"
        );
        // Unfloat returns the exception to tiled.
        let untiled = parse_reply(&planner.evaluate(&float_request(
            "float-seq-7",
            "win-2",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 100, 300, 720, 480, true),
                ("win-3", 400, 0, 100, 80, false),
                ("win-4", 600, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "toggle-float", "window": "win-2"}),
        )));
        assert_eq!(untiled["outcome"], "planned", "{untiled}");
        assert_eq!(untiled["float_geometry"], serde_json::Value::Null);
        assert_geometry_covers(&untiled, &["win-1", "win-2", "win-3", "win-4"]);
        // A later post-removal observation still plans on the reunited topology.
        let removed = parse_reply(&planner.evaluate(&float_request(
            "float-seq-8",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 200, 0, 100, 80, false),
                ("win-3", 400, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(removed["outcome"], "planned", "{removed}");
        assert_geometry_covers(&removed, &["win-1", "win-2", "win-3"]);
        // Stale/failure handling stays fail-closed for malformed shapes, but a
        // complete observation with changed membership converges first: the
        // missing member is removed and the reconcile projects the survivors.
        let partial = parse_reply(&planner.evaluate(&float_request(
            "float-seq-9",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 200, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(partial["outcome"], "planned", "{partial}");
        assert_eq!(partial["detail"]["kind"], "reconcile", "{partial}");
        assert_geometry_covers(&partial, &["win-1", "win-2"]);
        let converged = parse_reply(&planner.evaluate(&float_request(
            "float-seq-10",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80, false),
                ("win-2", 200, 0, 100, 80, false),
                ("win-3", 400, 0, 100, 80, false),
            ],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(converged["outcome"], "planned", "{converged}");
        assert_geometry_covers(&converged, &["win-1", "win-2", "win-3"]);
    }

    /// Reconcile request with explicit domain bounds and per-window rects, so
    /// portrait/landscape/square targets can carry deliberately misleading
    /// (opposite-orientation) observed window geometry.
    fn custom_request(
        correlation: &str,
        focused: &str,
        domain_rect: (i32, i32, i32, i32),
        windows: &[(&str, i32, i32, i32, i32)],
        command: serde_json::Value,
    ) -> String {
        let entries: Vec<serde_json::Value> = windows
            .iter()
            .map(|(window, x, y, w, h)| {
                serde_json::json!({
                    "window": window,
                    "output": "out-1",
                    "workspace": "ws-1",
                    "rect": {"x": x, "y": y, "w": w, "h": h},
                })
            })
            .collect();
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": "owner-1",
            "generation": "gen-1",
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": {"x": domain_rect.0, "y": domain_rect.1, "w": domain_rect.2, "h": domain_rect.3},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": entries,
            "command": command,
        })
        .to_string()
    }

    fn geometry_rects(reply: &serde_json::Value) -> Vec<(i32, i32, i32, i32)> {
        let mut rects: Vec<(i32, i32, i32, i32)> = reply["desired_geometry"]
            .as_array()
            .expect("planned geometry present")
            .iter()
            .map(|entry| {
                let rect = &entry["rect"];
                (
                    rect["x"].as_i64().unwrap() as i32,
                    rect["y"].as_i64().unwrap() as i32,
                    rect["w"].as_i64().unwrap() as i32,
                    rect["h"].as_i64().unwrap() as i32,
                )
            })
            .collect();
        rects.sort();
        rects
    }

    fn assert_geometry_covers(reply: &serde_json::Value, expected: &[&str]) {
        let geometry = reply["desired_geometry"]
            .as_array()
            .expect("planned geometry present");
        assert_eq!(geometry.len(), expected.len(), "{reply}");
        let mut got: Vec<String> = geometry
            .iter()
            .map(|g| g["window"].as_str().expect("window").to_owned())
            .collect();
        got.sort();
        let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(got, want, "{reply}");
        for entry in geometry {
            let rect = &entry["rect"];
            assert!(rect["w"].as_i64().unwrap() > 0, "{reply}");
            assert!(rect["h"].as_i64().unwrap() > 0, "{reply}");
            for key in ["window", "leaf", "output", "workspace"] {
                assert!(
                    entry[key].as_str().is_some_and(|s| !s.is_empty()),
                    "{reply}"
                );
            }
        }
        assert!(reply["desired_focus"]["leaf"].as_str().is_some(), "{reply}");
    }

    #[test]
    fn reconcile_fresh_portrait_output_splits_top_bottom_despite_landscape_rects() {
        // The split axis derives from the complete observation fit on the
        // portrait target: x intervals overlap (horizontal unsupported) while
        // y intervals stay sequential, so the vertical near-strip projects
        // top/bottom. Both observed rects stay landscape, which must not
        // select a left/right split on this portrait output. Product fit
        // policy is unchanged (fixed Horizontal tie-break when both axes
        // support; here only vertical supports).
        let request = custom_request(
            "admit-portrait-1",
            "win-1",
            (0, 0, 800, 1200),
            &[("win-1", 0, 0, 800, 300), ("win-2", 0, 300, 800, 300)],
            serde_json::json!({"op": "reconcile"}),
        );
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_rects(&reply),
            vec![(0, 0, 800, 600), (0, 600, 800, 600)],
            "{reply}"
        );
    }

    #[test]
    fn reconcile_fresh_landscape_output_splits_left_right_despite_portrait_rects() {
        // Portrait observed rects must not select a
        // top/bottom split on a landscape output.
        let request = custom_request(
            "admit-landscape-1",
            "win-1",
            (0, 0, 1200, 800),
            &[("win-1", 0, 0, 300, 400), ("win-2", 300, 400, 300, 400)],
            serde_json::json!({"op": "reconcile"}),
        );
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_rects(&reply),
            vec![(0, 0, 600, 800), (600, 0, 600, 800)],
            "{reply}"
        );
    }

    #[test]
    fn admit_reflows_an_out_of_bounds_member_after_planner_restart() {
        // Fresh complete reconcile is the admission route: an older
        // out-of-work-area member must not wedge a fresh domain.
        let command = serde_json::json!({"op": "reconcile"});
        // This is the reported shape: an older Firefox member extends 26px
        // below the work area while a new Firefox window is admitted.
        let request = custom_request(
            "admit-oob-1",
            "ghostty",
            (0, 44, 1536, 980),
            &[
                ("ghostty", 0, 44, 504, 980),
                ("firefox-old", 504, 44, 526, 1006),
                ("firefox-new", 1030, 44, 506, 980),
            ],
            command,
        );
        let mut first_planner = Planner::new();
        let first = parse_reply(&first_planner.evaluate(&request));
        assert_eq!(first["outcome"], "planned", "{first}");
        assert_geometry_covers(&first, &["ghostty", "firefox-old", "firefox-new"]);
        for geometry in first["desired_geometry"].as_array().expect("geometry") {
            let rect = &geometry["rect"];
            assert!(rect["x"].as_i64().unwrap() >= 0, "{first}");
            assert!(rect["y"].as_i64().unwrap() >= 44, "{first}");
            assert!(
                rect["x"].as_i64().unwrap() + rect["w"].as_i64().unwrap() <= 1536,
                "{first}"
            );
            assert!(
                rect["y"].as_i64().unwrap() + rect["h"].as_i64().unwrap() <= 1024,
                "{first}"
            );
        }

        // The raw out-of-bounds observation contains no state that can wedge
        // a fresh Planner instance after an adapter or Planner restart.
        let restarted_request = request.replace("admit-oob-1", "admit-oob-2");
        let mut restarted_planner = Planner::new();
        let restarted = parse_reply(&restarted_planner.evaluate(&restarted_request));
        assert_eq!(restarted["outcome"], "planned", "{restarted}");
        assert_geometry_covers(&restarted, &["ghostty", "firefox-old", "firefox-new"]);

        // Out-of-bounds drift is host drift: it plans and converges to
        // canonical geometry.
        let drift = custom_request(
            "drift-oob-1",
            "ghostty",
            (0, 44, 1536, 980),
            &[
                ("ghostty", 0, 44, 504, 980),
                ("firefox-old", 504, 44, 526, 1006),
                ("firefox-new", 1030, 44, 506, 980),
            ],
            serde_json::json!({"op": "focus", "window": "ghostty", "direction": "left"}),
        );
        let drift_reply = parse_reply(&restarted_planner.evaluate(&drift));
        assert_eq!(drift_reply["outcome"], "planned", "{drift_reply}");
        assert_geometry_covers(&drift_reply, &["ghostty", "firefox-old", "firefox-new"]);
    }

    #[test]
    fn retained_out_of_bounds_reconcile_repeats_then_normal_ops_plan() {
        // Mid-drag workspace send leaves a valid but out-of-bounds observed
        // frame (54,586,756,478 over work area 0,44,1536,980): retained
        // reconcile must accept it as host drift, converge to canonical
        // planned geometry, repeat without wedging, and keep later normal
        // operations usable. Malformed rectangles still reject.
        fn assert_contained(reply: &serde_json::Value) {
            for geometry in reply["desired_geometry"].as_array().expect("geometry") {
                let rect = &geometry["rect"];
                let (x, y, w, h) = (
                    rect["x"].as_i64().unwrap(),
                    rect["y"].as_i64().unwrap(),
                    rect["w"].as_i64().unwrap(),
                    rect["h"].as_i64().unwrap(),
                );
                assert!(x >= 0 && y >= 44, "{reply}");
                assert!(x + w <= 1536 && y + h <= 1024, "{reply}");
            }
        }
        let oob_windows = [
            ("ghostty", 54, 586, 756, 478),
            ("firefox-old", 810, 44, 363, 980),
            ("firefox-new", 1173, 44, 363, 980),
        ];
        let mut planner = Planner::new();
        let seed = custom_request(
            "oob-retained-seed-1",
            "ghostty",
            (0, 44, 1536, 980),
            &[
                ("ghostty", 0, 44, 512, 980),
                ("firefox-old", 512, 44, 512, 980),
                ("firefox-new", 1024, 44, 512, 980),
            ],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&seed))["outcome"], "planned");
        for cid in ["oob-retained-rec-1", "oob-retained-rec-2"] {
            let request = custom_request(
                cid,
                "ghostty",
                (0, 44, 1536, 980),
                &oob_windows,
                serde_json::json!({"op": "reconcile"}),
            );
            let reply = parse_reply(&planner.evaluate(&request));
            assert_eq!(reply["outcome"], "planned", "{reply}");
            assert_geometry_covers(&reply, &["ghostty", "firefox-old", "firefox-new"]);
            assert_contained(&reply);
        }
        let focus = custom_request(
            "oob-retained-focus-1",
            "firefox-old",
            (0, 44, 1536, 980),
            &oob_windows,
            serde_json::json!({"op": "focus", "window": "firefox-old", "direction": "left"}),
        );
        let focus_reply = parse_reply(&planner.evaluate(&focus));
        assert_eq!(focus_reply["outcome"], "planned", "{focus_reply}");
        assert_geometry_covers(&focus_reply, &["ghostty", "firefox-old", "firefox-new"]);
        assert_contained(&focus_reply);
        let normal = custom_request(
            "oob-retained-rec-3",
            "ghostty",
            (0, 44, 1536, 980),
            &[
                ("ghostty", 0, 44, 512, 980),
                ("firefox-old", 512, 44, 512, 980),
                ("firefox-new", 1024, 44, 512, 980),
            ],
            serde_json::json!({"op": "reconcile"}),
        );
        let normal_reply = parse_reply(&planner.evaluate(&normal));
        assert_eq!(normal_reply["outcome"], "planned", "{normal_reply}");
        assert_geometry_covers(&normal_reply, &["ghostty", "firefox-old", "firefox-new"]);
        let malformed = custom_request(
            "oob-retained-bad-1",
            "ghostty",
            (0, 44, 1536, 980),
            &[
                ("ghostty", 54, 586, 0, 478),
                ("firefox-old", 810, 44, 363, 980),
                ("firefox-new", 1173, 44, 363, 980),
            ],
            serde_json::json!({"op": "reconcile"}),
        );
        let malformed_reply = parse_reply(&planner.evaluate(&malformed));
        assert_eq!(malformed_reply["outcome"], "rejected", "{malformed_reply}");
        assert_eq!(
            malformed_reply["kind"], "snapshot-invalid",
            "{malformed_reply}"
        );
        assert_eq!(
            malformed_reply["detail"], "window-rect-invalid",
            "{malformed_reply}"
        );
    }

    #[test]
    fn rejection_kinds_are_bounded_without_echo() {
        // Surviving directional op with an unknown window id: the rejection
        // stays bounded and never echoes the raw native id.
        let bad = plan_request(
            "bounded-1",
            "win-1",
            &["win-1"],
            serde_json::json!({"op": "focus", "window": "evil-window-xyz", "direction": "left"}),
        );
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&bad));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        let text = serde_json::to_string(&reply).expect("serialize");
        assert!(!text.contains("evil-window-xyz"), "{reply}");
        assert!(text.len() <= PLAN_MAX_REPLY_BYTES, "{reply}");
    }

    fn retained_request(
        correlation: &str,
        owner: &str,
        generation: &str,
        focused: &str,
        windows: &[(&str, i32, i32, i32, i32)],
        command: serde_json::Value,
    ) -> String {
        let entries: Vec<serde_json::Value> = windows
            .iter()
            .map(|(window, x, y, w, h)| {
                serde_json::json!({
                    "window": window,
                    "output": "out-1",
                    "workspace": "ws-1",
                    "rect": {"x": x, "y": y, "w": w, "h": h},
                })
            })
            .collect();
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": owner,
            "generation": generation,
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": entries,
            "command": command,
        })
        .to_string()
    }

    fn retained_request_with_selected_gaps(
        correlation: &str,
        owner: &str,
        generation: &str,
        focused: &str,
        windows: &[(&str, i32, i32, i32, i32)],
        command: serde_json::Value,
    ) -> String {
        let mut request: serde_json::Value = serde_json::from_str(&retained_request(
            correlation,
            owner,
            generation,
            focused,
            windows,
            command,
        ))
        .expect("valid retained request");
        request["domain"]["gap"] = serde_json::json!(8);
        request["domain"]["outer_gap"] = serde_json::json!(8);
        request.to_string()
    }

    /// Retained request variant carrying AR12 per-window size hints:
    /// `hints` maps window id to `(min_size, max_size)` as `(w, h)` pairs.
    /// Windows absent from the map carry no hint fields (legacy shape).
    type HintPair = (Option<(i32, i32)>, Option<(i32, i32)>);
    /// Expected tile: window plus x/y/w/h and whether it flags overconstrained.
    type TileExpect<'a> = (&'a str, i32, i32, i32, i32, bool);
    #[allow(clippy::too_many_arguments)]
    fn retained_request_with_hints(
        correlation: &str,
        owner: &str,
        generation: &str,
        focused: &str,
        windows: &[(&str, i32, i32, i32, i32)],
        hints: &std::collections::BTreeMap<&str, HintPair>,
        command: serde_json::Value,
    ) -> String {
        let entries: Vec<serde_json::Value> = windows
            .iter()
            .map(|(window, x, y, w, h)| {
                let mut entry = serde_json::json!({
                    "window": window,
                    "output": "out-1",
                    "workspace": "ws-1",
                    "rect": {"x": x, "y": y, "w": w, "h": h},
                });
                if let Some((min, max)) = hints.get(window) {
                    if let Some((mw, mh)) = min {
                        entry["min_size"] = serde_json::json!({"w": mw, "h": mh});
                    }
                    if let Some((mw, mh)) = max {
                        entry["max_size"] = serde_json::json!({"w": mw, "h": mh});
                    }
                }
                entry
            })
            .collect();
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": owner,
            "generation": generation,
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": entries,
            "command": command,
        })
        .to_string()
    }

    #[test]
    fn retained_equal_non_focused_rects_plan_after_initial() {
        // After the initial plan the domain topology is retained, so equal
        // non-focused rectangles no longer force `ambiguous-placement`.
        let mut planner = Planner::new();
        for (correlation, focused, windows, command) in [
            (
                "d4-equal-1",
                "win-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "d4-equal-2",
                "win-1",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "d4-equal-3",
                "win-1",
                vec![
                    ("win-1", 0, 0, 100, 80),
                    ("win-2", 200, 0, 100, 80),
                    ("win-3", 400, 0, 100, 80),
                ],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let request =
                retained_request(correlation, "owner-1", "gen-1", focused, &windows, command);
            let reply = parse_reply(&planner.evaluate(&request));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        assert_eq!(planner.retained_domains(), 1);
        // Two non-focused windows share an exact frame; retained state
        // proposes directly from membership, so it still plans.
        let ambiguous = retained_request(
            "d4-equal-4",
            "owner-1",
            "gen-1",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 200, 0, 100, 80),
            ],
            serde_json::json!({"op": "reconcile"}),
        );
        let retained = parse_reply(&planner.evaluate(&ambiguous));
        assert_eq!(retained["outcome"], "planned", "{retained}");
        assert_geometry_covers(&retained, &["win-1", "win-2", "win-3"]);
        assert_eq!(planner.retained_domains(), 1);
    }

    #[test]
    fn retained_generation_change_discards_and_rebuilds() {
        // Recovery: owner/generation change (adapter restart) discards all
        // retained domains and rebuilds once from the fresh observation.
        let mut planner = Planner::new();
        let first = retained_request(
            "d4-gen-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&first))["outcome"], "planned");
        let second = retained_request(
            "d4-gen-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(
            parse_reply(&planner.evaluate(&second))["outcome"],
            "planned"
        );
        assert_eq!(planner.retained_domains(), 1);
        // New generation carries only win-1/win-2 with distinct frames: the
        // rebuild must succeed and must not leak gen-1 windows.
        let restarted = retained_request(
            "d4-gen-3",
            "owner-1",
            "gen-2",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let reply = parse_reply(&planner.evaluate(&restarted));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(planner.retained_domains(), 1);
        // Follow-up under the new generation still plans (no stale gen-1).
        let follow = retained_request(
            "d4-gen-4",
            "owner-1",
            "gen-2",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let follow_reply = parse_reply(&planner.evaluate(&follow));
        assert_eq!(follow_reply["outcome"], "planned", "{follow_reply}");
        assert_geometry_covers(&follow_reply, &["win-1"]);
    }

    #[test]
    fn retained_directional_rejects_tied_seed_without_retained_state() {
        // Fail-closed: a directional command with no retained topology that
        // cannot safely infer one (equal non-focused rects) rejects
        // `missing-seed-order` without retaining state; the next fresh
        // observation still recovers.
        let mut planner = Planner::new();
        let ambiguous = retained_request(
            "d4-reject-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 200, 0, 100, 80),
            ],
            serde_json::json!({"op": "move", "window": "win-1", "direction": "left"}),
        );
        let reply = parse_reply(&planner.evaluate(&ambiguous));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "snapshot-invalid", "{reply}");
        assert_eq!(reply["detail"], "missing-seed-order", "{reply}");
        assert_eq!(planner.retained_domains(), 0);
        let valid = retained_request(
            "d4-reject-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        // The next fresh observation still plans: no wedge, no retained base.
        let valid_reply = parse_reply(&planner.evaluate(&valid));
        assert_eq!(valid_reply["outcome"], "planned", "{valid_reply}");
        assert_geometry_covers(&valid_reply, &["win-1"]);
    }

    #[test]
    fn directional_edge_windows_use_raw_bounds_for_focus_and_move() {
        // Two horizontally adjacent outputs with a non-zero target origin and
        // an outer-gap inset: edge-tiled windows sit inside the raw work
        // areas but outside the inset projection. Directional containment
        // must use the raw bounds (like the single-domain route), while
        // planning still projects from the inset domains.
        fn reconcile_for(
            correlation: &str,
            output: &str,
            workspace: &str,
            bounds: serde_json::Value,
            focused: &str,
            windows: serde_json::Value,
        ) -> String {
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": 7,
                "domain": {
                    "output": output,
                    "workspace": workspace,
                    "bounds": bounds,
                    "gap": 8,
                    "outer_gap": 8,
                },
                "focused_window": focused,
                "windows": windows,
                "command": {"op": "reconcile"},
            })
            .to_string()
        }
        fn directional_for(correlation: &str, focused: &str, command: serde_json::Value) -> String {
            let domains = serde_json::json!([
                {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 44, "w": 2048, "h": 1108},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": {"right": "HDMI-A-2"},
                },
                {
                    "output": "HDMI-A-2",
                    "workspace": "ws-1",
                    "bounds": {"x": 2048, "y": 116, "w": 1920, "h": 1036},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": {"left": "DP-6"},
                },
            ]);
            let windows = serde_json::json!([
                {
                    "window": "win-1",
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "rect": {"x": 0, "y": 44, "w": 1024, "h": 1108},
                },
                {
                    "window": "win-1b",
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "rect": {"x": 1024, "y": 44, "w": 1024, "h": 1108},
                },
                {
                    "window": "win-2",
                    "output": "HDMI-A-2",
                    "workspace": "ws-1",
                    "rect": {"x": 2048, "y": 116, "w": 960, "h": 1036},
                },
            ]);
            let domain_entries: Vec<DirectionalDomainDto> =
                serde_json::from_value(domains.clone()).expect("domains decode");
            let window_entries: Vec<ObservedDto> =
                serde_json::from_value(windows.clone()).expect("windows decode");
            let fingerprint = directional_fingerprint(&domain_entries, focused, &window_entries);
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": fingerprint,
                "domain": {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 44, "w": 2048, "h": 1108},
                    "gap": 8,
                    "outer_gap": 8,
                },
                "domains": domains,
                "focused_window": focused,
                "windows": windows,
                "command": command,
            })
            .to_string()
        }
        fn seed_pair() -> Planner {
            let mut planner = Planner::new();
            let source_windows = serde_json::json!([
                {
                    "window": "win-1",
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "rect": {"x": 0, "y": 44, "w": 1024, "h": 1108},
                },
                {
                    "window": "win-1b",
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "rect": {"x": 1024, "y": 44, "w": 1024, "h": 1108},
                },
            ]);
            let target_windows = serde_json::json!([
                {
                    "window": "win-2",
                    "output": "HDMI-A-2",
                    "workspace": "ws-1",
                    "rect": {"x": 2048, "y": 116, "w": 960, "h": 1036},
                },
            ]);
            let source = parse_reply(&planner.evaluate(&reconcile_for(
                "edge-raw-seed-1",
                "DP-6",
                "ws-1",
                serde_json::json!({"x": 0, "y": 44, "w": 2048, "h": 1108}),
                "win-1",
                source_windows,
            )));
            assert_eq!(source["outcome"], "planned", "{source}");
            let target = parse_reply(&planner.evaluate(&reconcile_for(
                "edge-raw-seed-2",
                "HDMI-A-2",
                "ws-1",
                serde_json::json!({"x": 2048, "y": 116, "w": 1920, "h": 1036}),
                "win-2",
                target_windows,
            )));
            assert_eq!(target["outcome"], "planned", "{target}");
            planner
        }
        let mut focus_planner = seed_pair();
        let focus = parse_reply(&focus_planner.evaluate(&directional_for(
            "edge-raw-focus-1",
            "win-1b",
            serde_json::json!({"op": "focus", "window": "win-1b", "direction": "right"}),
        )));
        assert_eq!(focus["outcome"], "planned", "{focus}");
        assert_eq!(focus["detail"]["kind"], "focus", "{focus}");
        assert_eq!(
            focus["detail"]["capability"], "directional-focus",
            "{focus}"
        );
        assert_eq!(focus["detail"]["direction"], "right", "{focus}");
        assert_eq!(focus["detail"]["to_window"], "win-1", "{focus}");
        assert_eq!(focus["desired_focus"]["domain_output"], "DP-6", "{focus}");
        let mut move_planner = seed_pair();
        let moved = parse_reply(&move_planner.evaluate(&directional_for(
            "edge-raw-move-1",
            "win-1",
            serde_json::json!({"op": "move", "window": "win-1", "direction": "right"}),
        )));
        assert_eq!(moved["outcome"], "planned", "{moved}");
        assert_eq!(moved["detail"]["kind"], "move", "{moved}");
        assert_eq!(moved["detail"]["direction"], "right", "{moved}");
        assert_eq!(
            moved["detail"]["capability"], "CrossOutputTransfer",
            "{moved}"
        );
        assert_eq!(moved["detail"]["rule"], "R4", "{moved}");
        assert_eq!(moved["operation"]["target_output"], "HDMI-A-2", "{moved}");
        assert_eq!(moved["operation"]["source_output"], "DP-6", "{moved}");
        assert_eq!(moved["operation"]["window"], "win-1", "{moved}");
        assert_eq!(moved["operation"]["target"], "occupied", "{moved}");
        assert_eq!(
            moved["desired_focus"]["domain_output"], "HDMI-A-2",
            "{moved}"
        );
        for reply in [&focus, &moved] {
            for entry in reply["desired_geometry"]
                .as_array()
                .expect("planned geometry present")
            {
                let output = entry["output"].as_str().expect("output");
                let (bx, by, bw, bh) = match output {
                    "DP-6" => (8, 52, 2032, 1092),
                    "HDMI-A-2" => (2056, 124, 1904, 1020),
                    _ => panic!("unexpected output {reply}"),
                };
                let rect = &entry["rect"];
                let (x, y, w, h) = (
                    rect["x"].as_i64().expect("x"),
                    rect["y"].as_i64().expect("y"),
                    rect["w"].as_i64().expect("w"),
                    rect["h"].as_i64().expect("h"),
                );
                assert!(
                    x >= bx && y >= by && x + w <= bx + bw && y + h <= by + bh,
                    "{reply} {entry}"
                );
            }
        }
        let mut oob_request: serde_json::Value = serde_json::from_str(&directional_for(
            "edge-raw-oob-1",
            "win-1b",
            serde_json::json!({"op": "focus", "window": "win-1b", "direction": "right"}),
        ))
        .expect("directional request");
        oob_request["windows"][1]["rect"]["w"] = serde_json::json!(1025);
        let oob_domains: Vec<DirectionalDomainDto> =
            serde_json::from_value(oob_request["domains"].clone()).expect("domains decode");
        let oob_windows: Vec<ObservedDto> =
            serde_json::from_value(oob_request["windows"].clone()).expect("windows decode");
        oob_request["fingerprint"] = serde_json::json!(directional_fingerprint(
            &oob_domains,
            "win-1b",
            &oob_windows
        ));
        let mut oob_planner = seed_pair();
        let oob = parse_reply(&oob_planner.evaluate(&oob_request.to_string()));
        assert_eq!(oob["outcome"], "planned", "{oob}");
    }

    #[test]
    fn float_subject_cross_output_focus_plans_remembered_target() {
        // Float-origin Left/Right miss with no local float target: the
        // adapter searches native float geometry locally, so the wire only
        // carries the float_subject marker into the shared cross-output
        // remembered-target fallback. Seeds mirror the directional edge
        // test, then win-1b floats on the source output.
        fn reconcile_for(
            correlation: &str,
            output: &str,
            workspace: &str,
            bounds: serde_json::Value,
            focused: &str,
            windows: serde_json::Value,
        ) -> String {
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": 7,
                "domain": {
                    "output": output,
                    "workspace": workspace,
                    "bounds": bounds,
                    "gap": 8,
                    "outer_gap": 8,
                },
                "focused_window": focused,
                "windows": windows,
                "command": {"op": "reconcile"},
            })
            .to_string()
        }
        fn directional_for(
            correlation: &str,
            focused: &str,
            command: serde_json::Value,
            windows: serde_json::Value,
        ) -> String {
            let domains = serde_json::json!([
                {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 44, "w": 2048, "h": 1108},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": {"right": "HDMI-A-2"},
                },
                {
                    "output": "HDMI-A-2",
                    "workspace": "ws-1",
                    "bounds": {"x": 2048, "y": 116, "w": 1920, "h": 1036},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": {"left": "DP-6"},
                },
            ]);
            let domain_entries: Vec<DirectionalDomainDto> =
                serde_json::from_value(domains.clone()).expect("domains decode");
            let window_entries: Vec<ObservedDto> =
                serde_json::from_value(windows.clone()).expect("windows decode");
            let fingerprint = directional_fingerprint(&domain_entries, focused, &window_entries);
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": fingerprint,
                "domain": {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 44, "w": 2048, "h": 1108},
                    "gap": 8,
                    "outer_gap": 8,
                },
                "domains": domains,
                "focused_window": focused,
                "windows": windows,
                "command": command,
            })
            .to_string()
        }
        fn seed_pair() -> Planner {
            let mut planner = Planner::new();
            let source = parse_reply(&planner.evaluate(&reconcile_for(
                "float-cross-seed-1",
                "DP-6",
                "ws-1",
                serde_json::json!({"x": 0, "y": 44, "w": 2048, "h": 1108}),
                "win-1",
                serde_json::json!([
                    {"window": "win-1", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 0, "y": 44, "w": 1024, "h": 1108}},
                    {"window": "win-1b", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 1024, "y": 44, "w": 1024, "h": 1108}},
                ]),
            )));
            assert_eq!(source["outcome"], "planned", "{source}");
            let target = parse_reply(&planner.evaluate(&reconcile_for(
                "float-cross-seed-2",
                "HDMI-A-2",
                "ws-1",
                serde_json::json!({"x": 2048, "y": 116, "w": 1920, "h": 1036}),
                "win-2",
                serde_json::json!([
                    {"window": "win-2", "output": "HDMI-A-2", "workspace": "ws-1", "rect": {"x": 2048, "y": 116, "w": 960, "h": 1036}},
                ]),
            )));
            assert_eq!(target["outcome"], "planned", "{target}");
            // Float win-1b on the source output (focused while floating).
            let floated = parse_reply(&planner.evaluate(&reconcile_for(
                "float-cross-seed-3",
                "DP-6",
                "ws-1",
                serde_json::json!({"x": 0, "y": 44, "w": 2048, "h": 1108}),
                "win-1b",
                serde_json::json!([
                    {"window": "win-1", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 0, "y": 44, "w": 1024, "h": 1108}},
                    {"window": "win-1b", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 1024, "y": 44, "w": 1024, "h": 1108}},
                ]),
            )));
            assert_eq!(floated["outcome"], "planned", "{floated}");
            let toggled = parse_reply(
                &planner.evaluate(
                    &serde_json::json!({
                        "v": 1,
                        "correlation_id": "float-cross-seed-4",
                        "owner": "owner-1",
                        "generation": "gen-1",
                        "revision": 0,
                        "fingerprint": 7,
                        "domain": {
                            "output": "DP-6",
                            "workspace": "ws-1",
                            "bounds": {"x": 0, "y": 44, "w": 2048, "h": 1108},
                            "gap": 8,
                            "outer_gap": 8,
                        },
                        "focused_window": "win-1b",
                        "windows": [
                            {"window": "win-1", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 0, "y": 44, "w": 1024, "h": 1108}},
                            {"window": "win-1b", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 1024, "y": 44, "w": 1024, "h": 1108}},
                        ],
                        "command": {"op": "toggle-float", "window": "win-1b"},
                    })
                    .to_string(),
                ),
            );
            assert_eq!(toggled["outcome"], "planned", "{toggled}");
            planner
        }
        fn float_windows() -> serde_json::Value {
            serde_json::json!([
                {"window": "win-1", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 0, "y": 44, "w": 1024, "h": 1108}},
                {"window": "win-1b", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 1024, "y": 44, "w": 1024, "h": 1108}, "floating": true, "fit_excluded": true},
                {"window": "win-2", "output": "HDMI-A-2", "workspace": "ws-1", "rect": {"x": 2048, "y": 116, "w": 960, "h": 1036}},
            ])
        }
        // The flagged miss plans onto the adjacent output's remembered tile
        // with the float as the recorded source; no geometry is fabricated
        // for the float itself.
        let mut planner = seed_pair();
        let crossed = parse_reply(&planner.evaluate(&directional_for(
            "float-cross-1",
            "win-1b",
            serde_json::json!({"op": "focus", "window": "win-1b", "direction": "right", "float_subject": true}),
            float_windows(),
        )));
        assert_eq!(crossed["outcome"], "planned", "{crossed}");
        assert_eq!(crossed["detail"]["kind"], "focus", "{crossed}");
        assert_eq!(crossed["detail"]["to_window"], "win-2", "{crossed}");
        assert_eq!(crossed["operation"]["from_window"], "win-1b", "{crossed}");
        assert_eq!(
            crossed["operation"]["from_leaf"],
            serde_json::Value::Null,
            "{crossed}"
        );
        assert_eq!(
            crossed["preconditions"][0], "focused-floating-window",
            "{crossed}"
        );
        assert_eq!(crossed["operation"]["to_window"], "win-2", "{crossed}");
        assert_eq!(
            crossed["operation"]["cross_source_output"], "DP-6",
            "{crossed}"
        );
        assert_eq!(
            crossed["desired_focus"]["domain_output"], "HDMI-A-2",
            "{crossed}"
        );
        for entry in crossed["desired_geometry"].as_array().expect("geometry") {
            assert_ne!(entry["window"], "win-1b", "{crossed}");
        }
        // Omitted flag keeps tile-origin behavior: a floating subject
        // without the marker refuses as not-tiled, never crossing.
        let mut unflagged = seed_pair();
        let refused = parse_reply(&unflagged.evaluate(&directional_for(
            "float-cross-2",
            "win-1b",
            serde_json::json!({"op": "focus", "window": "win-1b", "direction": "right"}),
            float_windows(),
        )));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "not-tiled", "{refused}");
        // A tiled subject carrying the marker refuses as a focus mismatch.
        let mut tiled_flagged = seed_pair();
        let mismatch = parse_reply(&tiled_flagged.evaluate(&directional_for(
            "float-cross-3",
            "win-1",
            serde_json::json!({"op": "focus", "window": "win-1", "direction": "right", "float_subject": true}),
            float_windows(),
        )));
        assert_eq!(mismatch["outcome"], "rejected", "{mismatch}");
        assert_eq!(mismatch["kind"], "focus-mismatch", "{mismatch}");
        // The marker rides Left/Right two-domain requests only: Up refuses
        // as a malformed focus op, single-domain flagged requests refuse as
        // domain-invalid, and a non-boolean marker refuses malformed.
        let mut up = seed_pair();
        let up_refused = parse_reply(&up.evaluate(&directional_for(
            "float-cross-4",
            "win-1b",
            serde_json::json!({"op": "focus", "window": "win-1b", "direction": "up", "float_subject": true}),
            float_windows(),
        )));
        assert_eq!(up_refused["outcome"], "rejected", "{up_refused}");
        assert_eq!(up_refused["kind"], "snapshot-invalid", "{up_refused}");
        assert_eq!(up_refused["detail"], "focus-op-invalid", "{up_refused}");
        let mut single = seed_pair();
        let single_refused = parse_reply(
            &single.evaluate(
                &serde_json::json!({
                    "v": 1,
                    "correlation_id": "float-cross-5",
                    "owner": "owner-1",
                    "generation": "gen-1",
                    "revision": 0,
                    "fingerprint": 7,
                    "domain": {
                        "output": "DP-6",
                        "workspace": "ws-1",
                        "bounds": {"x": 0, "y": 44, "w": 2048, "h": 1108},
                        "gap": 8,
                        "outer_gap": 8,
                    },
                    "focused_window": "win-1b",
                    "windows": [
                        {"window": "win-1", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 0, "y": 44, "w": 1024, "h": 1108}},
                        {"window": "win-1b", "output": "DP-6", "workspace": "ws-1", "rect": {"x": 1024, "y": 44, "w": 1024, "h": 1108}, "floating": true, "fit_excluded": true},
                    ],
                    "command": {"op": "focus", "window": "win-1b", "direction": "right", "float_subject": true},
                })
                .to_string(),
            ),
        );
        assert_eq!(single_refused["outcome"], "rejected", "{single_refused}");
        assert_eq!(
            single_refused["kind"], "snapshot-invalid",
            "{single_refused}"
        );
        assert_eq!(
            single_refused["detail"], "domain-invalid",
            "{single_refused}"
        );
        let mut typed = seed_pair();
        let malformed = parse_reply(&typed.evaluate(&directional_for(
            "float-cross-6",
            "win-1b",
            serde_json::json!({"op": "focus", "window": "win-1b", "direction": "right", "float_subject": "yes"}),
            float_windows(),
        )));
        assert_eq!(malformed["outcome"], "rejected", "{malformed}");
        assert_eq!(malformed["kind"], "request-malformed", "{malformed}");
    }

    #[test]
    fn retained_ordinary_activation_resyncs_focus_for_directional() {
        // Part A: ordinary KWin activation changes only the observed
        // `focused_window`; retained Session focus otherwise only moves via
        // planned commands. A directional command carrying the other valid
        // tiled focus with complete matching membership must plan without a
        // generation change, and the follow-up must remain usable.
        let mut planner = Planner::new();
        let first = retained_request(
            "d4-focus-sync-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&first))["outcome"], "planned");
        let second = retained_request(
            "d4-focus-sync-2",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(
            parse_reply(&planner.evaluate(&second))["outcome"],
            "planned"
        );
        // Retained focus is now win-2. Ordinary activation moves observed
        // focus to win-1; membership is complete and unchanged.
        let activated = retained_request(
            "d4-focus-sync-3",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "focus", "window": "win-1", "direction": "right"}),
        );
        let reply = parse_reply(&planner.evaluate(&activated));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        // Committed resync: follow-up from the new focus stays usable.
        let follow = retained_request(
            "d4-focus-sync-4",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "focus", "window": "win-2", "direction": "left"}),
        );
        let follow_reply = parse_reply(&planner.evaluate(&follow));
        assert_eq!(follow_reply["outcome"], "planned", "{follow_reply}");
        assert_geometry_covers(&follow_reply, &["win-1", "win-2"]);
    }

    #[test]
    fn retained_keyboard_resize_caps_large_press_index() {
        // Retained `op=resize` must apply the COSMIC
        // `(10 + 2 + 2 * press_index).min(20)` cap for every u32.
        fn resize_shares(press_index: u32) -> serde_json::Value {
            let mut planner = Planner::new();
            for (correlation, focused, windows, command) in [
                (
                    "d6-resize-cap-1",
                    "win-1",
                    vec![("win-1", 0, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
                (
                    "d6-resize-cap-2",
                    "win-2",
                    vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
            ] {
                let request =
                    retained_request(correlation, "owner-1", "gen-1", focused, &windows, command);
                assert_eq!(
                    parse_reply(&planner.evaluate(&request))["outcome"],
                    "planned"
                );
            }
            let request = retained_request(
                "d6-resize-cap-3",
                "owner-1",
                "gen-1",
                "win-2",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({
                    "op": "resize",
                    "window": "win-2",
                    "direction": "left",
                    "mode": "outwards",
                    "press_index": press_index,
                }),
            );
            parse_reply(&planner.evaluate(&request))
        }
        let capped = resize_shares(4);
        assert_eq!(capped["outcome"], "planned", "{capped}");
        assert_geometry_covers(&capped, &["win-1", "win-2"]);
        for press_index in [2_147_483_648u32, u32::MAX] {
            let reply = resize_shares(press_index);
            assert_eq!(reply["outcome"], "planned", "{reply}");
            assert_geometry_covers(&reply, &["win-1", "win-2"]);
            assert_eq!(
                reply["detail"]["new_shares"], capped["detail"]["new_shares"],
                "press_index {press_index} must cap at 20px like press_index 4: {reply} vs {capped}"
            );
        }
    }

    fn seed_pointer_planner() -> Planner {
        let mut planner = Planner::new();
        for (correlation, focused, windows, command) in [
            (
                "ptr-seed-1",
                "win-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "ptr-seed-2",
                "win-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let request =
                retained_request(correlation, "owner-1", "gen-1", focused, &windows, command);
            assert_eq!(
                parse_reply(&planner.evaluate(&request))["outcome"],
                "planned"
            );
        }
        planner
    }

    #[test]
    fn retained_pointer_resize_corner_commits_both_axes_in_one_request() {
        // Nested retained layout from two horizontal reconciles plus one tall
        // reconcile: root H [win-1 | V[win-2, win-3]] over 1200x800 with the
        // shared horizontal edge at 600 and the shared vertical edge at
        // 400, focused on win-3. One corner request must plan and commit
        // both axes atomically: a single planned reply whose geometry moves
        // both shared edges, advancing exactly one revision.
        let mut planner = seed_pointer_planner();
        let seed3 = retained_request(
            "ptr-seed-3",
            "owner-1",
            "gen-1",
            "win-3",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 300),
            ],
            serde_json::json!({"op": "reconcile"}),
        );
        let seeded = parse_reply(&planner.evaluate(&seed3));
        assert_eq!(seeded["outcome"], "planned", "{seeded}");
        let request = retained_request(
            "ptr-corner-1",
            "owner-1",
            "gen-1",
            "win-3",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 300),
            ],
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-3",
                "direction": "left",
                "boundary": 590,
                "direction2": "up",
                "boundary2": 390,
            }),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["base_revision"], 3, "{reply}");
        assert_eq!(reply["detail"]["kind"], "pointer-resize", "{reply}");
        assert_eq!(reply["detail"]["capability"], "pointer-resize", "{reply}");
        assert_eq!(reply["detail"]["direction"], "left", "{reply}");
        assert_eq!(reply["detail"]["boundary"], 590, "{reply}");
        assert_eq!(reply["detail"]["direction2"], "up", "{reply}");
        assert_eq!(reply["detail"]["boundary2"], 390, "{reply}");
        assert!(reply["detail"]["target_group"].is_string(), "{reply}");
        assert!(reply["detail"]["target_group2"].is_string(), "{reply}");
        assert_ne!(
            reply["detail"]["target_group"], reply["detail"]["target_group2"],
            "corner spans two split groups: {reply}"
        );
        let rect_of = |window: &str| {
            reply["desired_geometry"]
                .as_array()
                .expect("geometry")
                .iter()
                .find(|g| g["window"] == window)
                .expect("member")
                .get("rect")
                .expect("rect")
                .clone()
        };
        assert_eq!(rect_of("win-3")["x"], 590, "{reply}");
        assert_eq!(rect_of("win-3")["y"], 390, "{reply}");
        assert_eq!(
            rect_of("win-1")["x"].as_i64().unwrap() + rect_of("win-1")["w"].as_i64().unwrap(),
            590,
            "{reply}"
        );
        assert_eq!(
            rect_of("win-2")["y"].as_i64().unwrap() + rect_of("win-2")["h"].as_i64().unwrap(),
            390,
            "{reply}"
        );
        assert_geometry_covers(&reply, &["win-1", "win-2", "win-3"]);
    }

    #[test]
    fn retained_pointer_resize_corner_refusals_are_exact() {
        let mut planner = seed_pointer_planner();
        let seed3 = retained_request(
            "ptr-seed-3",
            "owner-1",
            "gen-1",
            "win-3",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 300),
            ],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&seed3))["outcome"], "planned");
        let mut corner = |correlation: &str, command: serde_json::Value| {
            parse_reply(&planner.evaluate(&retained_request(
                correlation,
                "owner-1",
                "gen-1",
                "win-3",
                &[
                    ("win-1", 0, 0, 100, 80),
                    ("win-2", 200, 0, 100, 80),
                    ("win-3", 400, 0, 100, 300),
                ],
                command,
            )))
        };
        // Same-axis pair cannot express a corner.
        let same_axis = corner(
            "ptr-corner-bad-1",
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-3",
                "direction": "left",
                "boundary": 590,
                "direction2": "right",
                "boundary2": 700,
            }),
        );
        assert_eq!(same_axis["outcome"], "rejected", "{same_axis}");
        assert_eq!(same_axis["kind"], "direction-invalid", "{same_axis}");
        // Half-present pair is an op shape violation.
        let half = corner(
            "ptr-corner-bad-2",
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-3",
                "direction": "left",
                "boundary": 590,
                "direction2": "up",
            }),
        );
        assert_eq!(half["outcome"], "rejected", "{half}");
        assert_eq!(half["kind"], "snapshot-invalid", "{half}");
        assert_eq!(half["detail"], "pointer-resize-op-invalid", "{half}");
        // Unparsable second direction binds the direction refusal.
        let bad_dir = corner(
            "ptr-corner-bad-3",
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-3",
                "direction": "left",
                "boundary": 590,
                "direction2": "sideways",
                "boundary2": 390,
            }),
        );
        assert_eq!(bad_dir["outcome"], "rejected", "{bad_dir}");
        assert_eq!(bad_dir["kind"], "direction-invalid", "{bad_dir}");
        // No-op on either grabbed axis refuses the whole corner as
        // unchanged with no plan and no pending: the committed edge still
        // sits at 600/400 afterwards.
        let noop = corner(
            "ptr-corner-bad-4",
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-3",
                "direction": "left",
                "boundary": 600,
                "direction2": "up",
                "boundary2": 390,
            }),
        );
        assert_eq!(noop["outcome"], "rejected", "{noop}");
        assert_eq!(noop["kind"], "unchanged", "{noop}");
        assert!(noop.get("desired_geometry").is_none(), "{noop}");
    }

    #[test]
    fn retained_pointer_resize_reflows_allocation_and_shares() {
        let mut planner = seed_pointer_planner();
        let request = retained_request(
            "ptr-ok-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-2",
                "direction": "left",
                "boundary": 550,
            }),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["detail"]["kind"], "pointer-resize", "{reply}");
        assert_eq!(reply["detail"]["capability"], "pointer-resize", "{reply}");
        assert_eq!(reply["detail"]["direction"], "left", "{reply}");
        assert_eq!(reply["detail"]["boundary"], 550, "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        let old = reply["detail"]["old_shares"]
            .as_array()
            .expect("old shares");
        let new = reply["detail"]["new_shares"]
            .as_array()
            .expect("new shares");
        assert_ne!(old, new, "{reply}");
    }

    #[test]
    fn retained_pointer_resize_out_of_bounds_refuses_precisely() {
        let mut planner = seed_pointer_planner();
        let request = retained_request(
            "ptr-bad-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-2",
                "direction": "left",
                "boundary": 5000,
            }),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "malformed-input", "{reply}");
        assert_eq!(
            reply["message"], "command or observation input is malformed",
            "{reply}"
        );
        assert!(reply.get("detail").is_none(), "{reply}");
        assert!(reply.get("desired_geometry").is_none(), "{reply}");
    }

    #[test]
    fn retained_pointer_resize_refusal_details_are_exact() {
        let mut planner = seed_pointer_planner();
        // Unknown window: valid opaque id covered by the observation but not
        // tiled in the session.
        let request = retained_request(
            "ptr-bad-2",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-zzz",
                "direction": "left",
                "boundary": 150,
            }),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "unknown-window", "{reply}");
        assert_eq!(
            reply["message"], "window is not known to the session",
            "{reply}"
        );
        assert!(reply.get("desired_geometry").is_none(), "{reply}");
        // Invalid direction binds the exact direction refusal.
        let request = retained_request(
            "ptr-bad-3",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-2",
                "direction": "sideways",
                "boundary": 150,
            }),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "direction-invalid", "{reply}");
        assert_eq!(reply["message"], "direction is invalid", "{reply}");
        assert!(reply.get("desired_geometry").is_none(), "{reply}");
    }

    fn base_valid_value(cid: &str) -> serde_json::Value {
        serde_json::from_str(&plan_request(
            cid,
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "move", "window": "win-1", "direction": "left"}),
        ))
        .expect("valid base")
    }

    fn assert_retained_detail(mut value: serde_json::Value, cid: &str, expected: &str) {
        value["correlation_id"] = serde_json::json!(cid);
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "snapshot-invalid", "{reply}");
        assert_eq!(reply["detail"], expected, "{reply}");
    }

    #[test]
    fn snapshot_validation_details_are_exact() {
        let cases: Vec<(&str, SnapshotMutator)> = vec![
            ("domain-output-invalid", |v| {
                v["domain"]["output"] = serde_json::json!("bad id!")
            }),
            ("domain-workspace-invalid", |v| {
                v["domain"]["workspace"] = serde_json::json!("bad id!")
            }),
            ("focused-id-invalid", |v| {
                v["focused_window"] = serde_json::json!("bad id!")
            }),
            ("observed-window-invalid", |v| {
                v["windows"][0]["window"] = serde_json::json!("bad!")
            }),
            ("observed-output-invalid", |v| {
                v["windows"][0]["output"] = serde_json::json!("bad!")
            }),
            ("observed-workspace-invalid", |v| {
                v["windows"][0]["workspace"] = serde_json::json!("bad!")
            }),
            ("duplicate-window", |v| {
                v["windows"][1]["window"] = serde_json::json!("win-1");
                v["windows"][1]["rect"] = serde_json::json!({"x": 400, "y": 0, "w": 100, "h": 80});
            }),
            ("domain-bounds-invalid", |v| {
                v["domain"]["bounds"]["w"] = serde_json::json!(0)
            }),
            ("gap-low", |v| v["domain"]["gap"] = serde_json::json!(-1)),
            ("gap-high", |v| v["domain"]["gap"] = serde_json::json!(65)),
            ("outer-gap-low", |v| {
                v["domain"]["outer_gap"] = serde_json::json!(-1)
            }),
            ("outer-gap-high", |v| {
                v["domain"]["outer_gap"] = serde_json::json!(65)
            }),
            ("window-rect-invalid", |v| {
                v["windows"][0]["rect"]["w"] = serde_json::json!(0)
            }),
            ("focused-not-observed", |v| {
                v["focused_window"] = serde_json::json!("win-9")
            }),
            ("inset-exhausted", |v| {
                v["domain"]["bounds"] = serde_json::json!({"x": 0, "y": 0, "w": 10, "h": 10});
                v["domain"]["outer_gap"] = serde_json::json!(5);
                v["windows"] = serde_json::json!([]);
                v["focused_window"] = serde_json::json!("");
            }),
        ];
        for (index, (expected, mutate)) in cases.into_iter().enumerate() {
            let mut value = base_valid_value(&format!("snap-v-{index}"));
            mutate(&mut value);
            assert_retained_detail(value, &format!("snap-v-{index}"), expected);
        }
    }

    #[test]
    fn many_observed_windows_plan_without_count_cap() {
        // No observed-window count cap: 70 windows in one horizontal strip
        // reconcile through the flat N-ary fit path on a fresh planner. Each
        // carried rectangle is valid, contained, and strictly sequential.
        let names: Vec<String> = (0..70).map(|i| format!("win-{i}")).collect();
        let windows: Vec<(&str, i32, i32, i32, i32)> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.as_str(), i as i32 * 17, 0, 10, 800))
            .collect();
        let focused = names.last().expect("names").clone();
        let command = serde_json::json!({"op": "reconcile"});
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "many-win-1",
            "owner-1",
            "gen-1",
            &focused,
            &windows,
            command,
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(70),
            "{reply}"
        );
    }
    #[test]
    fn command_and_construction_details_are_exact() {
        let cases: Vec<(&str, serde_json::Value)> = vec![
            (
                "move-window-invalid",
                serde_json::json!({"op": "move", "window": "bad!", "direction": "left"}),
            ),
            (
                "focus-window-invalid",
                serde_json::json!({"op": "focus", "window": "bad!", "direction": "left"}),
            ),
            (
                "resize-window-invalid",
                serde_json::json!({"op": "resize", "window": "bad!", "direction": "left", "mode": "outwards", "press_index": 0}),
            ),
        ];
        for (index, (expected, command)) in cases.into_iter().enumerate() {
            let mut value = base_valid_value(&format!("snap-c-{index}"));
            value["command"] = command;
            assert_retained_detail(value, &format!("snap-c-{index}"), expected);
        }
        let ambiguous = retained_request(
            "snap-c-amb",
            "owner-1",
            "gen-1",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 200, 0, 100, 80),
            ],
            serde_json::json!({"op": "move", "window": "win-1", "direction": "left"}),
        );
        let mut planner = Planner::new();
        let retained = parse_reply(&planner.evaluate(&ambiguous));
        assert_eq!(retained["kind"], "snapshot-invalid", "{retained}");
        assert_eq!(retained["detail"], "missing-seed-order", "{retained}");
    }
    #[test]
    fn retained_op_details_are_exact() {
        let cases = [
            (
                "move-op-invalid",
                serde_json::json!({"op": "move", "window": "win-1", "direction": "left"}),
                Planner::evaluate_move_retained as fn(&mut Planner, &Validated) -> String,
            ),
            (
                "focus-op-invalid",
                serde_json::json!({"op": "focus", "window": "win-1", "direction": "left"}),
                Planner::evaluate_focus_retained,
            ),
            (
                "resize-op-invalid",
                serde_json::json!({"op": "resize", "window": "win-1", "direction": "left", "mode": "outwards", "press_index": 0}),
                Planner::evaluate_resize_retained,
            ),
        ];
        for (index, (expected, command, eval)) in cases.into_iter().enumerate() {
            let cid = format!("snap-o-ret-{index}");
            let mut ctx =
                validate_request(&plan_request(&cid, "win-1", &["win-1", "win-2"], command))
                    .expect("base valid");
            ctx.request.command["op"] = serde_json::json!("bogus-op");
            let mut planner = Planner::new();
            let text = eval(&mut planner, &ctx);
            let reply = parse_reply(&text);
            assert_eq!(reply["detail"], expected, "{reply}");
            assert_eq!(reply["correlation_id"], cid, "{reply}");
            assert!(text.len() <= PLAN_MAX_REPLY_BYTES, "{reply}");
        }
    }
    #[test]
    fn typed_sync_codec_active_group_wire_golden() {
        // Wire golden for the typed `SyncCommand` active-group: valid requests
        // plan byte-exact through the single typed parse + inner path,
        // malformed commands reject byte-exact with unchanged kinds. Retired
        // admit/remove wire goldens deleted; seeding uses complete-observation
        // reconcile. Literals recorded from the production `evaluate` path
        // (offline, no host mutation).
        // Seed a two-window domain through complete observations.
        let mut planner = Planner::new();
        for (cid, focused, windows) in [
            ("s1", "win-1", vec![("win-1", 0, 0, 100, 80)]),
            (
                "s2",
                "win-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            ),
        ] {
            let reply = parse_reply(&planner.evaluate(&retained_request(
                cid,
                "owner-1",
                "gen-1",
                focused,
                &windows,
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        let active_group = retained_request(
            "gold-ag-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "active-group"}),
        );
        assert_eq!(
            planner.evaluate(&active_group),
            "{\"v\":1,\"correlation_id\":\"gold-ag-1\",\"outcome\":\"active-group\",\"kind\":\"active-group\",\"base_revision\":2,\"detail\":{\"bounds\":{\"h\":800,\"w\":1200,\"x\":0,\"y\":0},\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"focused_leaf\":\"leaf-win-1\",\"focused_window\":\"win-1\",\"generation\":\"gen-1\",\"group\":\"grp-win-2-r1\",\"kind\":\"active-group\",\"members\":[{\"leaf\":\"leaf-win-1\",\"rect\":{\"h\":800,\"w\":600,\"x\":0,\"y\":0},\"window\":\"win-1\"},{\"leaf\":\"leaf-win-2\",\"rect\":{\"h\":800,\"w\":600,\"x\":600,\"y\":0},\"window\":\"win-2\"}],\"owner\":\"owner-1\"},\"desired_geometry\":[{\"window\":\"win-1\",\"leaf\":\"leaf-win-1\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":0,\"y\":0,\"w\":600,\"h\":800}},{\"window\":\"win-2\",\"leaf\":\"leaf-win-2\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":600,\"y\":0,\"w\":600,\"h\":800}}],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-win-1\"}}",
        );
        let active_group_extra = retained_request(
            "gold-ag-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "active-group", "bogus": 1}),
        );
        assert_eq!(
            planner.evaluate(&active_group_extra),
            "{\"v\":1,\"correlation_id\":\"gold-ag-2\",\"outcome\":\"rejected\",\"kind\":\"unknown-field\",\"message\":\"request contains an unknown field\"}",
        );
    }
    #[test]
    fn typed_sync_codec_covers_all_eleven_ops_total() {
        // Fence proof for the boundary conversion: all eleven synchronous wire
        // ops decode once via `SyncCommand`, then convert into `CoreCommand`
        // with the identical `op` token. Fallible vocabularies
        // (direction/mode) cross opaquely. The eight retired wire ops
        // (ack/verify/status/cancel on both routes) no longer decode.
        let commands = [
            serde_json::json!({"op": "reconcile"}),
            serde_json::json!({"op": "update-gaps"}),
            serde_json::json!({"op": "active-group"}),
            serde_json::json!({"op": "move", "window": "win-1", "direction": "left"}),
            serde_json::json!({"op": "focus", "window": "win-1", "direction": "left"}),
            serde_json::json!({"op": "resize", "window": "win-1", "direction": "left", "mode": "inwards", "press_index": 0}),
            serde_json::json!({"op": "pointer-resize", "window": "win-1", "direction": "left", "boundary": 10}),
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
            serde_json::json!({"op": "send-to-workspace", "window": "win-1", "target_output": "out-1", "target_workspace": "ws-2"}),
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 610, "y": 400}),
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 610, "y": 400}),
        ];
        assert_eq!(commands.len(), 11);
        let mut ops = std::collections::HashSet::new();
        for command in &commands {
            let decoded: SyncCommand =
                serde_json::from_value(command.clone()).expect("wire op decodes");
            let converted = core_command_from_sync(&decoded).expect("sync op converts");
            let expected = command.get("op").and_then(|op| op.as_str()).expect("op");
            assert_eq!(converted.op(), expected);
            ops.insert(converted.op());
        }
        assert_eq!(ops.len(), 11);
        // Opaque crossing: an unknown direction string converts without
        // validation; handlers own precedence.
        let decoded: SyncCommand = serde_json::from_value(
            serde_json::json!({"op": "move", "window": "win-1", "direction": "sideways"}),
        )
        .expect("decodes");
        assert!(matches!(
            core_command_from_sync(&decoded).expect("sync op converts"),
            tiler_core::boundary::CoreCommand::Move { direction, .. } if direction == "sideways"
        ));
        // Retired wire ops no longer decode as sync commands.
        for retired in [
            serde_json::json!({"op": "send-to-workspace-ack", "ack_outcome": "accepted"}),
            serde_json::json!({"op": "send-to-workspace-verify", "verified": true, "preconditions": [], "operation": {}}),
            serde_json::json!({"op": "send-to-workspace-status"}),
            serde_json::json!({"op": "send-to-workspace-cancel", "zero_dispatch": false}),
            serde_json::json!({"op": "directional-move-ack", "ack_outcome": "accepted"}),
            serde_json::json!({"op": "directional-move-verify", "verified": true, "preconditions": [], "operation": {}}),
            serde_json::json!({"op": "directional-move-status"}),
            serde_json::json!({"op": "directional-move-cancel", "zero_dispatch": false}),
        ] {
            assert!(
                serde_json::from_value::<SyncCommand>(retired).is_err(),
                "retired op must not decode"
            );
        }
    }

    #[test]
    fn retired_lifecycle_commands_refuse_without_mutation() {
        let mut planner = Planner::new();
        for command in [
            serde_json::json!({"op": "admit", "window": "win-1", "output": "out-1", "workspace": "ws-1"}),
            serde_json::json!({"op": "remove", "window": "win-1"}),
            serde_json::json!({"op": "send-to-workspace-ack", "ack_outcome": "accepted"}),
            serde_json::json!({"op": "send-to-workspace-verify", "verified": true, "preconditions": [], "operation": {}}),
            serde_json::json!({"op": "send-to-workspace-status"}),
            serde_json::json!({"op": "send-to-workspace-cancel", "zero_dispatch": false}),
            serde_json::json!({"op": "directional-move-ack", "ack_outcome": "accepted"}),
            serde_json::json!({"op": "directional-move-verify", "verified": true, "preconditions": [], "operation": {}}),
            serde_json::json!({"op": "directional-move-status"}),
            serde_json::json!({"op": "directional-move-cancel", "zero_dispatch": false}),
        ] {
            let reply = parse_reply(&planner.evaluate(&retained_request(
                "retired-wire-1",
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80)],
                command,
            )));
            assert_eq!(reply["outcome"], "rejected", "{reply}");
            assert_eq!(reply["kind"], "unknown-value", "{reply}");
            assert_eq!(planner.retained_domains(), 0);
        }
    }

    #[test]
    fn core_reply_choke_point_matches_legacy_constructors_byte_exact() {
        // Proof that `serialize_core_reply` is a byte-exact funnel for the
        // surviving `CoreReply` shapes: each arm must equal its legacy
        // constructor. Production-real arms (Projection, fresh Tiled
        // admission, ActiveGroup/NoGroup) are additionally covered by wire
        // goldens through `evaluate`; the remaining arms pin bytes here
        // until their routes migrate.
        use tiler_core::boundary::{
            CoreReply, NoGroupReason, ProjectionKind, ProjectionPlan, TiledKind, TiledPlan,
        };
        use tiler_core::contract::DivergenceKind;
        let request = retained_request(
            "core-reply-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let ctx = validate_request(&request).expect("fixture validates");
        assert_eq!(
            serialize_core_reply(
                &ctx,
                &CoreReply::Rejected {
                    kind: "unknown-value",
                    message: MSG_UNKNOWN_VALUE,
                }
            ),
            rejected(
                "core-reply-1".to_owned(),
                "unknown-value",
                MSG_UNKNOWN_VALUE
            ),
        );
        assert_eq!(
            serialize_core_reply(
                &ctx,
                &CoreReply::SnapshotInvalid {
                    message: MSG_OBSERVATION,
                    detail: "focused-not-observed",
                }
            ),
            snapshot_invalid(
                "core-reply-1".to_owned(),
                MSG_OBSERVATION,
                "focused-not-observed",
            ),
        );
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::Diverged(DivergenceKind::OwnerMismatch)),
            diverged_reply("core-reply-1", DivergenceKind::OwnerMismatch),
        );
        let projection = ProjectionPlan {
            base_revision: 2,
            kind: ProjectionKind::Reconcile,
            geometry: Vec::new(),
            focus_domain: None,
            focus_leaf: None,
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::Projection(projection.clone())),
            planned_projection_reply("core-reply-1", &projection),
        );
        let tiled = TiledPlan {
            base_revision: 2,
            policy_version: 1,
            kind: TiledKind::Admit,
            geometry: Vec::new(),
            focus_domain: None,
            focus_leaf: None,
            float_window: None,
            float_rect: None,
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::Tiled(tiled.clone())),
            planned_tiled_reply("core-reply-1", &tiled),
        );
        assert_eq!(
            serialize_core_reply(
                &ctx,
                &CoreReply::NoGroup {
                    base_revision: None,
                    reason: NoGroupReason::NoSession,
                }
            ),
            no_group_reply(&ctx, None, "no-session"),
        );
    }
    #[test]
    fn sync_family_choke_point_matches_legacy_shapes_byte_exact() {
        // Byte pins for the migrated sync family (move local/cross, focus
        // local/cross, keyboard/pointer resize, toggle-float): literals
        // recorded from the typed serializers, with key order and tokens
        // verified against the replaced legacy constructors. The matching
        // production goldens (`typed_sync_codec_move_focus_resize_float_wire_golden`
        // and friends) prove the same bytes flow end to end.
        use tiler_core::boundary::{
            CoreReply, FocusPlanReply, MoveCrossView, MovePlanReply, ResizePlanReply, TiledKind,
            TiledPlan,
        };
        use tiler_core::contract::{FocusOperation, ResizeMode, ResizeOperation};
        use tiler_core::directional::{Capability, CrossOutputTarget, Direction, Rule};
        let request = retained_request(
            "core-sync-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let ctx = validate_request(&request).expect("fixture validates");
        let focus_leaf = NodeId::from("leaf-1");
        let move_local = MovePlanReply {
            base_revision: 2,
            rule: Rule::R2a,
            capability: Capability::SwapNeighbor,
            direction: Direction::Right,
            geometry: Vec::new(),
            focus_domain: ctx.domain_key.clone(),
            focus_leaf: focus_leaf.clone(),
            cross: None,
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::MoveDirectional(move_local)),
            "{\"v\":1,\"correlation_id\":\"core-sync-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"SwapNeighbor\",\"direction\":\"right\",\"kind\":\"move\",\"rule\":\"R2a\"},\"desired_geometry\":[],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-1\"}}",
        );
        let move_cross = MovePlanReply {
            base_revision: 2,
            rule: Rule::R2a,
            capability: Capability::SwapNeighbor,
            direction: Direction::Right,
            geometry: Vec::new(),
            focus_domain: ctx.domain_key.clone(),
            focus_leaf: focus_leaf.clone(),
            cross: Some(MoveCrossView {
                rule: Rule::R4,
                intent_direction: Direction::Right,
                intent_window: WindowId::from("win-1"),
                intent_leaf: NodeId::from("leaf-1"),
                source_output: OutputId::from("out-1"),
                source_workspace: WorkspaceId::from("ws-1"),
                target_output: OutputId::from("out-2"),
                target_workspace: WorkspaceId::from("ws-1"),
                source_root_child_index: 1,
                target: CrossOutputTarget::Occupied,
                preconditions: Vec::new(),
            }),
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::MoveDirectional(move_cross)),
            "{\"v\":1,\"correlation_id\":\"core-sync-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"SwapNeighbor\",\"direction\":\"right\",\"kind\":\"move\",\"rule\":\"R2a\"},\"desired_geometry\":[],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-1\"},\"preconditions\":[],\"operation\":{\"capability\":\"SwapNeighbor\",\"direction\":\"right\",\"leaf\":\"leaf-1\",\"op\":\"move\",\"rule\":\"R4\",\"source_output\":\"out-1\",\"source_root_child_index\":1,\"source_workspace\":\"ws-1\",\"target\":\"occupied\",\"target_output\":\"out-2\",\"target_workspace\":\"ws-1\",\"window\":\"win-1\"}}",
        );
        let focus_local = FocusPlanReply {
            base_revision: 2,
            direction: Direction::Right,
            to_window: WindowId::from("win-2"),
            geometry: Vec::new(),
            focus_domain: ctx.domain_key.clone(),
            focus_leaf: focus_leaf.clone(),
            cross_operation: None,
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::FocusDirectional(focus_local)),
            "{\"v\":1,\"correlation_id\":\"core-sync-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"directional-focus\",\"direction\":\"right\",\"kind\":\"focus\",\"to_window\":\"win-2\"},\"desired_geometry\":[],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-1\"}}",
        );
        let focus_cross = FocusPlanReply {
            base_revision: 2,
            direction: Direction::Right,
            to_window: WindowId::from("win-2"),
            geometry: Vec::new(),
            focus_domain: ctx.domain_key.clone(),
            focus_leaf: focus_leaf.clone(),
            cross_operation: Some(FocusOperation {
                domain_output: OutputId::from("out-2"),
                domain_workspace: WorkspaceId::from("ws-1"),
                from_leaf: Some(NodeId::from("a")),
                to_leaf: NodeId::from("b"),
                from_window: WindowId::from("win-1"),
                to_window: WindowId::from("win-2"),
                direction: Direction::Right,
                route: vec![NodeId::from("a"), NodeId::from("b")],
                cross_source_output: Some(OutputId::from("out-1")),
                cross_source_workspace: Some(WorkspaceId::from("ws-1")),
            }),
        };
        let cross_text = serialize_core_reply(&ctx, &CoreReply::FocusDirectional(focus_cross));
        let cross: serde_json::Value = serde_json::from_str(&cross_text).expect("serializes");
        assert_eq!(cross["outcome"], "planned");
        assert_eq!(
            cross["detail"],
            serde_json::json!({"capability": "directional-focus", "cross_output": true, "direction": "right", "kind": "focus", "to_window": "win-2"}),
        );
        assert_eq!(
            cross["operation"],
            serde_json::json!({"op": "focus", "domain_output": "out-2", "domain_workspace": "ws-1", "from_leaf": "a", "to_leaf": "b", "from_window": "win-1", "to_window": "win-2", "direction": "right", "route": ["a", "b"], "cross_source_output": "out-1", "cross_source_workspace": "ws-1"}),
        );
        let resize_op = ResizeOperation {
            domain_output: OutputId::from("out-1"),
            domain_workspace: WorkspaceId::from("ws-1"),
            focused_leaf: NodeId::from("a"),
            focused_window: WindowId::from("win-1"),
            direction: Direction::Right,
            mode: ResizeMode::Outwards,
            target_group: NodeId::from("grp"),
            focused_child: NodeId::from("a"),
            neighbor_child: NodeId::from("b"),
            focused_index: 0,
            neighbor_index: 1,
            old_shares: vec![1, 1],
            new_shares: vec![611, 587],
        };
        let resize = ResizePlanReply {
            base_revision: 2,
            direction: Direction::Right,
            mode: Some(ResizeMode::Outwards),
            boundary: None,
            operation: resize_op.clone(),
            secondary: None,
            geometry: Vec::new(),
            focus_domain: ctx.domain_key.clone(),
            focus_leaf: focus_leaf.clone(),
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::Resize(resize)),
            "{\"v\":1,\"correlation_id\":\"core-sync-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"keyboard-resize\",\"direction\":\"right\",\"focused_index\":0,\"kind\":\"resize\",\"mode\":\"outwards\",\"neighbor_index\":1,\"new_shares\":[611,587],\"old_shares\":[1,1],\"target_group\":\"grp\"},\"desired_geometry\":[],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-1\"}}",
        );
        let pointer = ResizePlanReply {
            base_revision: 2,
            direction: Direction::Right,
            mode: None,
            boundary: Some(120),
            operation: resize_op,
            secondary: None,
            geometry: Vec::new(),
            focus_domain: ctx.domain_key.clone(),
            focus_leaf: focus_leaf.clone(),
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::Resize(pointer)),
            "{\"v\":1,\"correlation_id\":\"core-sync-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"boundary\":120,\"capability\":\"pointer-resize\",\"direction\":\"right\",\"focused_index\":0,\"kind\":\"pointer-resize\",\"neighbor_index\":1,\"new_shares\":[611,587],\"old_shares\":[1,1],\"target_group\":\"grp\"},\"desired_geometry\":[],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-1\"}}",
        );
        let float_tiled = TiledPlan {
            base_revision: 2,
            policy_version: 1,
            kind: TiledKind::ToggleFloat,
            geometry: Vec::new(),
            focus_domain: None,
            focus_leaf: None,
            float_window: Some(WindowId::from("win-1")),
            float_rect: Some(Rect {
                x: 240,
                y: 160,
                w: 720,
                h: 480,
            }),
        };
        assert_eq!(
            serialize_core_reply(&ctx, &CoreReply::Tiled(float_tiled)),
            "{\"v\":1,\"correlation_id\":\"core-sync-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"intentional-float\",\"kind\":\"toggle-float\",\"policy_version\":1},\"desired_geometry\":[],\"float_geometry\":{\"window\":\"win-1\",\"rect\":{\"x\":240,\"y\":160,\"w\":720,\"h\":480}}}",
        );
    }
    #[test]
    fn typed_sync_codec_move_focus_resize_float_wire_golden() {
        // Wire golden for the tagged `SyncCommand` conversion of
        // move/focus/resize/pointer-resize/toggle-float: valid requests reply
        // byte-exact through the in-place typed parse, malformed commands
        // reject byte-exact with unchanged kinds. Literals recorded from the
        // production `evaluate` path before the switch (offline).
        // Explicit `cross_output_transfer: false` refuses the local swap as
        // `planner-noop` while the omitted (default-true) form plans: pins
        // the preserved default. Each group below runs on a freshly seeded
        // two-window planner so committed plans cannot bleed across goldens.
        let seed = || {
            let mut planner = Planner::new();
            for (cid, focused, windows, command) in [
                (
                    "s1",
                    "win-1",
                    vec![("win-1", 0, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
                (
                    "s2",
                    "win-2",
                    vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
            ] {
                let reply = parse_reply(&planner.evaluate(&retained_request(
                    cid, "owner-1", "gen-1", focused, &windows, command,
                )));
                assert_eq!(reply["outcome"], "planned", "{reply}");
            }
            planner
        };
        let mut planner = seed();
        let two = vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        let request = retained_request(
            "gold-move-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "move", "window": "win-1", "direction": "right"}),
        );
        assert_eq!(
            planner.evaluate(&request),
            "{\"v\":1,\"correlation_id\":\"gold-move-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"SwapNeighbor\",\"direction\":\"right\",\"kind\":\"move\",\"rule\":\"R2a\"},\"desired_geometry\":[{\"window\":\"win-1\",\"leaf\":\"leaf-win-1\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":600,\"y\":0,\"w\":600,\"h\":800}},{\"window\":\"win-2\",\"leaf\":\"leaf-win-2\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":0,\"y\":0,\"w\":600,\"h\":800}}],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-win-1\"}}",
        );
        let no_transfer = retained_request(
            "gold-move-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "move", "window": "win-1", "direction": "right", "cross_output_transfer": false}),
        );
        assert_eq!(
            planner.evaluate(&no_transfer),
            "{\"v\":1,\"correlation_id\":\"gold-move-2\",\"outcome\":\"rejected\",\"kind\":\"planner-noop\",\"message\":\"planner reports no movement\"}",
        );
        let move_extra = retained_request(
            "gold-move-3",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "move", "window": "win-1", "direction": "right", "bogus": 1}),
        );
        assert_eq!(
            planner.evaluate(&move_extra),
            "{\"v\":1,\"correlation_id\":\"gold-move-3\",\"outcome\":\"rejected\",\"kind\":\"unknown-field\",\"message\":\"request contains an unknown field\"}",
        );
        planner = seed();
        let focus = retained_request(
            "gold-focus-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "focus", "window": "win-1", "direction": "right"}),
        );
        assert_eq!(
            planner.evaluate(&focus),
            "{\"v\":1,\"correlation_id\":\"gold-focus-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"directional-focus\",\"direction\":\"right\",\"kind\":\"focus\",\"to_window\":\"win-2\"},\"desired_geometry\":[{\"window\":\"win-1\",\"leaf\":\"leaf-win-1\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":0,\"y\":0,\"w\":600,\"h\":800}},{\"window\":\"win-2\",\"leaf\":\"leaf-win-2\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":600,\"y\":0,\"w\":600,\"h\":800}}],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-win-2\"}}",
        );
        let focus_missing = retained_request(
            "gold-focus-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "focus", "window": "win-2"}),
        );
        assert_eq!(
            planner.evaluate(&focus_missing),
            "{\"v\":1,\"correlation_id\":\"gold-focus-2\",\"outcome\":\"rejected\",\"kind\":\"request-malformed\",\"message\":\"request is malformed\"}",
        );
        planner = seed();
        let resize = retained_request(
            "gold-resize-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "resize", "window": "win-1", "direction": "right", "mode": "outwards", "press_index": 0}),
        );
        assert_eq!(
            planner.evaluate(&resize),
            "{\"v\":1,\"correlation_id\":\"gold-resize-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"keyboard-resize\",\"direction\":\"right\",\"focused_index\":0,\"kind\":\"resize\",\"mode\":\"outwards\",\"neighbor_index\":1,\"new_shares\":[611,587],\"old_shares\":[1,1],\"target_group\":\"grp-win-2-r1\"},\"desired_geometry\":[{\"window\":\"win-1\",\"leaf\":\"leaf-win-1\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":0,\"y\":0,\"w\":612,\"h\":800}},{\"window\":\"win-2\",\"leaf\":\"leaf-win-2\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":612,\"y\":0,\"w\":588,\"h\":800}}],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-win-1\"}}",
        );
        let resize_extra = retained_request(
            "gold-resize-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "resize", "window": "win-1", "direction": "right", "mode": "outwards", "press_index": 0, "bogus": 1}),
        );
        assert_eq!(
            planner.evaluate(&resize_extra),
            "{\"v\":1,\"correlation_id\":\"gold-resize-2\",\"outcome\":\"rejected\",\"kind\":\"unknown-field\",\"message\":\"request contains an unknown field\"}",
        );
        planner = seed();
        let pointer = retained_request(
            "gold-ptr-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "pointer-resize", "window": "win-1", "direction": "right", "boundary": 550}),
        );
        assert_eq!(
            planner.evaluate(&pointer),
            "{\"v\":1,\"correlation_id\":\"gold-ptr-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"boundary\":550,\"capability\":\"pointer-resize\",\"direction\":\"right\",\"focused_index\":0,\"kind\":\"pointer-resize\",\"neighbor_index\":1,\"new_shares\":[549,649],\"old_shares\":[1,1],\"target_group\":\"grp-win-2-r1\"},\"desired_geometry\":[{\"window\":\"win-1\",\"leaf\":\"leaf-win-1\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":0,\"y\":0,\"w\":550,\"h\":800}},{\"window\":\"win-2\",\"leaf\":\"leaf-win-2\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":550,\"y\":0,\"w\":650,\"h\":800}}],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-win-1\"}}",
        );
        let pointer_missing = retained_request(
            "gold-ptr-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "pointer-resize", "window": "win-1", "direction": "right"}),
        );
        assert_eq!(
            planner.evaluate(&pointer_missing),
            "{\"v\":1,\"correlation_id\":\"gold-ptr-2\",\"outcome\":\"rejected\",\"kind\":\"request-malformed\",\"message\":\"request is malformed\"}",
        );
        planner = seed();
        let float = retained_request(
            "gold-float-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
        );
        assert_eq!(
            planner.evaluate(&float),
            "{\"v\":1,\"correlation_id\":\"gold-float-1\",\"outcome\":\"planned\",\"base_revision\":2,\"detail\":{\"capability\":\"intentional-float\",\"kind\":\"toggle-float\",\"policy_version\":1},\"desired_geometry\":[{\"window\":\"win-2\",\"leaf\":\"leaf-win-2\",\"output\":\"out-1\",\"workspace\":\"ws-1\",\"rect\":{\"x\":0,\"y\":0,\"w\":1200,\"h\":800}}],\"desired_focus\":{\"domain_output\":\"out-1\",\"domain_workspace\":\"ws-1\",\"leaf\":\"leaf-win-2\"},\"float_geometry\":{\"window\":\"win-1\",\"rect\":{\"x\":240,\"y\":160,\"w\":720,\"h\":480}}}",
        );
        let float_extra = retained_request(
            "gold-float-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "toggle-float", "window": "win-1", "bogus": 1}),
        );
        assert_eq!(
            planner.evaluate(&float_extra),
            "{\"v\":1,\"correlation_id\":\"gold-float-2\",\"outcome\":\"rejected\",\"kind\":\"unknown-field\",\"message\":\"request contains an unknown field\"}",
        );
        // Probe-before-parse precedence: a malformed float naming an untracked
        // floating window still reports `not-tiled`, never `unknown-field`.
        let mut fresh = Planner::new();
        let mut floating_value: serde_json::Value = serde_json::from_str(&retained_request(
            "gold-float-3",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "toggle-float", "window": "win-1", "bogus": 1}),
        ))
        .expect("request JSON");
        floating_value["windows"][0]["floating"] = serde_json::Value::Bool(true);
        assert_eq!(
            fresh.evaluate(&floating_value.to_string()),
            "{\"v\":1,\"correlation_id\":\"gold-float-3\",\"outcome\":\"rejected\",\"kind\":\"not-tiled\",\"message\":\"focused window is not a tiled window\"}",
        );
    }

    #[test]
    fn typed_sync_codec_same_axis_move_default_variants_and_invalid() {
        // R-MOV-03 wire contract: a missing `same_axis_move` field decodes to
        // group-with-neighbor (the explicit form replies identically);
        // `swap-with-neighbor` decodes and leaves the binary R2a rule
        // unchanged on this pair; unknown values (including the retired
        // `cosmic-wrap`/`flat-swap` tokens) refuse as snapshot-invalid
        // `move-op-invalid`; non-string values refuse via the malformed path;
        // the KDE `sameAxisMove` camelCase spelling refuses as an unknown
        // field. Each planned case runs on a freshly seeded planner so
        // committed plans cannot bleed across cases.
        let seed = || {
            let mut planner = Planner::new();
            for (cid, focused, windows, command) in [
                (
                    "axis-s1",
                    "win-1",
                    vec![("win-1", 0, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
                (
                    "axis-s2",
                    "win-1",
                    vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
            ] {
                let reply = parse_reply(&planner.evaluate(&retained_request(
                    cid, "owner-1", "gen-1", focused, &windows, command,
                )));
                assert_eq!(reply["outcome"], "planned", "{reply}");
            }
            planner
        };
        let two = vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        let move_command = |mode: Option<serde_json::Value>| {
            let mut command =
                serde_json::json!({"op": "move", "window": "win-1", "direction": "right"});
            if let Some(mode) = mode {
                command["same_axis_move"] = mode;
            }
            command
        };
        let planned_rule = |reply: &str| {
            let value = parse_reply(reply);
            assert_eq!(value["outcome"], "planned", "{value}");
            assert_eq!(value["detail"]["kind"], "move", "{value}");
            value["detail"]["rule"].as_str().expect("rule").to_owned()
        };
        // Missing field plans the historical wrap behavior (R2a here).
        let missing = retained_request(
            "axis-missing-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            move_command(None),
        );
        assert_eq!(planned_rule(&seed().evaluate(&missing)), "R2a");
        // Explicit group-with-neighbor replies identically (modulo correlation).
        let explicit = retained_request(
            "axis-explicit-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            move_command(Some(serde_json::json!("group-with-neighbor"))),
        );
        let missing_reply = parse_reply(&seed().evaluate(&missing));
        let explicit_reply = parse_reply(&seed().evaluate(&explicit));
        assert_eq!(missing_reply["outcome"], "planned", "{missing_reply}");
        assert_eq!(explicit_reply["outcome"], "planned", "{explicit_reply}");
        for key in ["detail", "desired_geometry", "desired_focus"] {
            assert_eq!(explicit_reply[key], missing_reply[key], "{key}");
        }
        // Swap-with-neighbor decodes and leaves the binary R2a rule unchanged.
        let flat = retained_request(
            "axis-flat-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            move_command(Some(serde_json::json!("swap-with-neighbor"))),
        );
        assert_eq!(planned_rule(&seed().evaluate(&flat)), "R2a");
        // Unknown values refuse as snapshot-invalid move-op-invalid.
        let invalid = retained_request(
            "axis-invalid-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            move_command(Some(serde_json::json!("diagonal-wrap"))),
        );
        assert_eq!(
            seed().evaluate(&invalid),
            "{\"v\":1,\"correlation_id\":\"axis-invalid-1\",\"outcome\":\"rejected\",\"kind\":\"snapshot-invalid\",\"message\":\"request contains an unknown value\",\"detail\":\"move-op-invalid\"}",
        );
        // Retired pre-release tokens refuse identically (no aliases).
        for (cid, token) in [
            ("axis-retired-1", "cosmic-wrap"),
            ("axis-retired-2", "flat-swap"),
        ] {
            let retired = retained_request(
                cid,
                "owner-1",
                "gen-1",
                "win-1",
                &two,
                move_command(Some(serde_json::json!(token))),
            );
            assert_eq!(
                seed().evaluate(&retired),
                format!(
                    "{{\"v\":1,\"correlation_id\":\"{cid}\",\"outcome\":\"rejected\",\"kind\":\"snapshot-invalid\",\"message\":\"request contains an unknown value\",\"detail\":\"move-op-invalid\"}}"
                ),
            );
        }
        // Non-string values refuse via the malformed path.
        let mistyped = retained_request(
            "axis-mistyped-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            move_command(Some(serde_json::json!(1))),
        );
        assert_eq!(
            seed().evaluate(&mistyped),
            "{\"v\":1,\"correlation_id\":\"axis-mistyped-1\",\"outcome\":\"rejected\",\"kind\":\"request-malformed\",\"message\":\"request is malformed\"}",
        );
        // The camelCase spelling is not the wire field.
        let camel = retained_request(
            "axis-camel-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "move", "window": "win-1", "direction": "right", "sameAxisMove": "swap-with-neighbor"}),
        );
        assert_eq!(
            seed().evaluate(&camel),
            "{\"v\":1,\"correlation_id\":\"axis-camel-1\",\"outcome\":\"rejected\",\"kind\":\"unknown-field\",\"message\":\"request contains an unknown field\"}",
        );
    }

    #[test]
    fn typed_sync_codec_same_axis_nary_wrap_vs_swap() {
        // R-MOV-03 N-ary discrimination at the wire: incremental admission of
        // four side-by-side windows nests binary (session-fixture shape), a
        // focus-left walk reaches win-1, and one right move grows the inner
        // group (R2b). Focusing up then moving down hits R2c. Missing and explicit
        // `group-with-neighbor` wrap (`WrapSiblings`); `swap-with-neighbor`
        // swaps (`SwapNeighbor`); both keep the mover focused.
        let four = vec![
            ("win-1", 0, 0, 300, 800),
            ("win-2", 300, 0, 300, 800),
            ("win-3", 600, 0, 300, 800),
            ("win-4", 900, 0, 300, 800),
        ];
        let focus_cmd = |window: &str, direction: &str| serde_json::json!({"op": "focus", "window": window, "direction": direction});
        let move_cmd = |window: &str, direction: &str, mode: Option<&str>| {
            let mut command = serde_json::json!({
                "op": "move",
                "window": window,
                "direction": direction,
            });
            if let Some(mode) = mode {
                command["same_axis_move"] = serde_json::json!(mode);
            }
            command
        };
        // Full fixture replay on a fresh planner; only the final move
        // carries the variant under test.
        let replay = |correlation: &str, mode: Option<&str>| {
            let mut planner = Planner::new();
            for n in 1..=4usize {
                let focused = format!("win-{n}");
                let reply = parse_reply(&planner.evaluate(&retained_request(
                    &format!("{correlation}-seed-{n}"),
                    "owner-1",
                    "gen-1",
                    &focused,
                    &four[..n],
                    serde_json::json!({"op": "reconcile"}),
                )));
                assert_eq!(reply["outcome"], "planned", "{reply}");
            }
            let mut focused = "win-4".to_owned();
            for step in 0..4 {
                let reply = parse_reply(&planner.evaluate(&retained_request(
                    &format!("{correlation}-focus-{step}"),
                    "owner-1",
                    "gen-1",
                    &focused,
                    &four,
                    focus_cmd(&focused, "left"),
                )));
                assert_eq!(reply["outcome"], "planned", "{reply}");
                focused = reply["desired_focus"]["leaf"]
                    .as_str()
                    .expect("focus leaf")
                    .strip_prefix("leaf-")
                    .expect("leaf prefix")
                    .to_owned();
                if focused == "win-1" {
                    break;
                }
            }
            assert_eq!(focused, "win-1", "fixture must reach win-1");
            let setup = parse_reply(&planner.evaluate(&retained_request(
                &format!("{correlation}-setup"),
                "owner-1",
                "gen-1",
                "win-1",
                &four,
                move_cmd("win-1", "right", None),
            )));
            assert_eq!(setup["outcome"], "planned", "{setup}");
            assert_eq!(setup["detail"]["rule"], "R2b", "{setup}");
            // The R2b insertion leaves V[win-2, win-1, H[win-3, win-4]], so
            // focus up to win-2 and move it down onto its leaf neighbor: the
            // R2c case discriminating wrap from swap.
            let focused_up = parse_reply(&planner.evaluate(&retained_request(
                &format!("{correlation}-focus-up"),
                "owner-1",
                "gen-1",
                "win-1",
                &four,
                focus_cmd("win-1", "up"),
            )));
            assert_eq!(focused_up["outcome"], "planned", "{focused_up}");
            assert_eq!(
                focused_up["desired_focus"]["leaf"], "leaf-win-2",
                "{focused_up}"
            );
            parse_reply(&planner.evaluate(&retained_request(
                &format!("{correlation}-final"),
                "owner-1",
                "gen-1",
                "win-2",
                &four,
                move_cmd("win-2", "down", mode),
            )))
        };
        let missing = replay("nary-missing", None);
        assert_eq!(missing["outcome"], "planned", "{missing}");
        assert_eq!(missing["detail"]["rule"], "R2c", "{missing}");
        assert_eq!(missing["detail"]["direction"], "down", "{missing}");
        assert_eq!(missing["detail"]["capability"], "WrapSiblings", "{missing}");
        assert_eq!(missing["desired_focus"]["leaf"], "leaf-win-2", "{missing}");
        let explicit = replay("nary-explicit", Some("group-with-neighbor"));
        assert_eq!(explicit["outcome"], "planned", "{explicit}");
        for key in ["detail", "desired_geometry", "desired_focus"] {
            assert_eq!(explicit[key], missing[key], "{key}");
        }
        let flat = replay("nary-flat", Some("swap-with-neighbor"));
        assert_eq!(flat["outcome"], "planned", "{flat}");
        assert_eq!(flat["detail"]["rule"], "R2c", "{flat}");
        assert_eq!(flat["detail"]["capability"], "SwapNeighbor", "{flat}");
        assert_eq!(flat["desired_focus"]["leaf"], "leaf-win-2", "{flat}");
        // The swap exchanges the leaf slots while the wrap nests the pair,
        // so the mover rect differs between the two.
        let mover_rect = |reply: &serde_json::Value| {
            reply["desired_geometry"]
                .as_array()
                .expect("geometry")
                .iter()
                .find(|g| g["window"] == "win-2")
                .expect("mover geometry")
                .get("rect")
                .expect("rect")
                .clone()
        };
        assert_ne!(
            mover_rect(&flat),
            mover_rect(&missing),
            "{flat} vs {missing}"
        );
    }

    #[test]
    fn same_axis_move_codec_decodes_default_and_converts_typed() {
        // Codec unit pins: missing `same_axis_move` deserializes to the
        // `group-with-neighbor` default; both wire tokens decode; an unknown
        // string (including retired `cosmic-wrap`/`flat-swap`) still decodes
        // as a string (the handler refuses it, never the codec);
        // `core_command_from_sync` maps valid tokens to the typed enum and
        // returns `None` (never panics) for anything else.
        let decode = |command: serde_json::Value| {
            let decoded: SyncCommand = serde_json::from_value(command).expect("move shape decodes");
            match decoded {
                SyncCommand::Move { same_axis_move, .. } => same_axis_move,
                other => panic!("expected move, got {other:?}"),
            }
        };
        assert_eq!(
            decode(serde_json::json!({"op": "move", "window": "w", "direction": "right"})),
            "group-with-neighbor"
        );
        assert_eq!(
            decode(
                serde_json::json!({"op": "move", "window": "w", "direction": "right", "same_axis_move": "group-with-neighbor"})
            ),
            "group-with-neighbor"
        );
        assert_eq!(
            decode(
                serde_json::json!({"op": "move", "window": "w", "direction": "right", "same_axis_move": "swap-with-neighbor"})
            ),
            "swap-with-neighbor"
        );
        assert_eq!(
            decode(
                serde_json::json!({"op": "move", "window": "w", "direction": "right", "same_axis_move": "diagonal-wrap"})
            ),
            "diagonal-wrap"
        );
        let convert = |mode: &str| {
            core_command_from_sync(&SyncCommand::Move {
                window: "w".to_owned(),
                direction: "right".to_owned(),
                cross_output_transfer: true,
                same_axis_move: mode.to_owned(),
            })
        };
        assert!(matches!(
            convert("group-with-neighbor"),
            Some(tiler_core::boundary::CoreCommand::Move {
                same_axis_move: SameAxisMove::GroupWithNeighbor,
                ..
            })
        ));
        assert!(matches!(
            convert("swap-with-neighbor"),
            Some(tiler_core::boundary::CoreCommand::Move {
                same_axis_move: SameAxisMove::SwapWithNeighbor,
                ..
            })
        ));
        assert_eq!(convert("diagonal-wrap"), None);
        assert_eq!(convert(""), None);
        assert_eq!(convert("cosmic-wrap"), None);
        assert_eq!(convert("flat-swap"), None);
        for input in [
            serde_json::json!({"op": "move", "window": "w", "direction": "right"}),
            serde_json::json!({"op": "move", "window": "w", "direction": "right", "same_axis_move": "group-with-neighbor"}),
            serde_json::json!({"op": "move", "window": "w", "direction": "right", "same_axis_move": "swap-with-neighbor"}),
        ] {
            let decoded: SyncCommand = serde_json::from_value(input.clone()).expect("move decodes");
            let Some(tiler_core::boundary::CoreCommand::Move { same_axis_move, .. }) =
                core_command_from_sync(&decoded)
            else {
                panic!("typed move expected");
            };
            let encoded = serde_json::json!({
                "op": "move", "window": "w", "direction": "right",
                "same_axis_move": same_axis_move.as_wire_str(),
            });
            assert_eq!(
                encoded["same_axis_move"],
                input
                    .get("same_axis_move")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!("group-with-neighbor"))
            );
            let round_trip: SyncCommand = serde_json::from_value(encoded).expect("move round trip");
            assert_eq!(
                core_command_from_sync(&round_trip),
                core_command_from_sync(&decoded)
            );
        }
        // Wire encoding is the validated enum token in both directions.
        assert_eq!(
            SameAxisMove::GroupWithNeighbor.as_wire_str(),
            "group-with-neighbor"
        );
        assert_eq!(
            SameAxisMove::SwapWithNeighbor.as_wire_str(),
            "swap-with-neighbor"
        );
        assert_eq!(SameAxisMove::default(), SameAxisMove::GroupWithNeighbor);
    }

    #[test]
    fn fixed_size_predicate_default_variants_and_invalid() {
        // R-SPC-04 D1 wire contract: omitted `fixed_size_predicate` decodes
        // to `both-axes-fixed`; both tokens parse; unknown values (including
        // camelCase) refuse as snapshot-invalid `fixed-predicate-invalid`
        // before any state change.
        use tiler_core::size_hints::FixedSizePredicate;
        assert_eq!(FixedSizePredicate::default(), FixedSizePredicate::BothAxes);
        assert_eq!(
            FixedSizePredicate::BothAxes.as_wire_str(),
            "both-axes-fixed"
        );
        assert_eq!(
            FixedSizePredicate::EitherAxis.as_wire_str(),
            "either-axis-fixed"
        );
        assert_eq!(
            FixedSizePredicate::parse_wire("both-axes-fixed"),
            Some(FixedSizePredicate::BothAxes)
        );
        assert_eq!(
            FixedSizePredicate::parse_wire("either-axis-fixed"),
            Some(FixedSizePredicate::EitherAxis)
        );
        for invalid in [
            "",
            "both",
            "either",
            "fixed",
            "fixedSizePredicate",
            "both_axes_fixed",
        ] {
            assert_eq!(FixedSizePredicate::parse_wire(invalid), None, "{invalid:?}");
        }
        let base = retained_request(
            "pred-missing-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&base));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        // Explicit both-axes replies identically to omitted (modulo correlation).
        let mut explicit_value: serde_json::Value = serde_json::from_str(&base).expect("valid");
        explicit_value["fixed_size_predicate"] = serde_json::json!("both-axes-fixed");
        let reply2 = parse_reply(&planner.evaluate(&explicit_value.to_string()));
        assert_eq!(reply2["outcome"], "planned", "{reply2}");
        // Unknown predicate refuses fail-closed with no state change.
        let mut invalid_value: serde_json::Value = serde_json::from_str(&base).expect("valid");
        invalid_value["fixed_size_predicate"] = serde_json::json!("bogus");
        assert_eq!(
            Planner::new().evaluate(&invalid_value.to_string()),
            "{\"v\":1,\"correlation_id\":\"pred-missing-1\",\"outcome\":\"rejected\",\"kind\":\"snapshot-invalid\",\"message\":\"request contains an unknown value\",\"detail\":\"fixed-predicate-invalid\"}",
        );
        // camelCase spelling is not the wire field.
        let mut camel: serde_json::Value = serde_json::from_str(&base).expect("valid");
        camel["fixedSizePredicate"] = serde_json::json!("either-axis-fixed");
        assert_eq!(
            Planner::new().evaluate(&camel.to_string()),
            "{\"v\":1,\"correlation_id\":\"pred-missing-1\",\"outcome\":\"rejected\",\"kind\":\"unknown-field\",\"message\":\"request contains an unknown field\"}",
        );
    }

    #[test]
    fn fixed_size_predicate_either_axis_admits_one_axis_with_switch_only_subsequent() {
        // One retained planner across a predicate switch: one-axis-fixed
        // tiles under the default, the switch to `either-axis-fixed` keeps
        // retained tiled windows tiled while a new one-axis client floats
        // with no tiled geometry, switching back preserves the automatic
        // float, and an explicit user tile override wins afterwards.
        let hints = |window: &str| {
            serde_json::json!({
                "window": window,
                "output": "out-1",
                "workspace": "ws-1",
                "rect": {"x": 0, "y": 0, "w": 100, "h": 80},
                "min_size": {"w": 640, "h": 100},
                "max_size": {"w": 640, "h": 480},
            })
        };
        let floating_hints = |window: &str| {
            let mut value = hints(window);
            value["floating"] = serde_json::json!(true);
            value["fit_excluded"] = serde_json::json!(true);
            value
        };
        let suppressed_hints = |window: &str| {
            let mut value = hints(window);
            value["fixed_suppress"] = serde_json::json!(true);
            value
        };
        let request_with =
            |correlation: &str, windows: serde_json::Value, predicate: Option<&str>| {
                let mut request = serde_json::json!({
                    "v": 1,
                    "correlation_id": correlation,
                    "owner": "owner-1",
                    "generation": "gen-1",
                    "revision": 0,
                    "fingerprint": 7,
                    "domain": {
                        "output": "out-1",
                        "workspace": "ws-1",
                        "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                        "gap": 0,
                        "outer_gap": 0,
                    },
                    "focused_window": "win-1",
                    "windows": windows,
                    "command": {"op": "reconcile"},
                });
                if let Some(predicate) = predicate {
                    request["fixed_size_predicate"] = serde_json::json!(predicate);
                }
                request.to_string()
            };
        let geometry_has = |reply: &serde_json::Value, window: &str| {
            reply["desired_geometry"]
                .as_array()
                .expect("geometry")
                .iter()
                .any(|g| g["window"] == window)
        };
        let mut planner = Planner::new();
        // Step 1, default predicate: one-axis-fixed tiles (settles tiled).
        let reply = parse_reply(&planner.evaluate(&request_with(
            "pred-switch-1",
            serde_json::json!([
                {"window": "win-1", "output": "out-1", "workspace": "ws-1", "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
                hints("win-2"),
            ]),
            None,
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert!(geometry_has(&reply, "win-1"), "{reply}");
        assert!(
            geometry_has(&reply, "win-2"),
            "one-axis tiles by default {reply}"
        );
        // Step 2, switched to either-axis on the SAME planner with the same
        // members plus a new one-axis client: retained win-2 stays tiled
        // (no reclassification) while brand-new win-3 floats (no geometry).
        let reply = parse_reply(&planner.evaluate(&request_with(
            "pred-switch-2",
            serde_json::json!([
                {"window": "win-1", "output": "out-1", "workspace": "ws-1", "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
                hints("win-2"),
                hints("win-3"),
            ]),
            Some("either-axis-fixed"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert!(geometry_has(&reply, "win-1"), "{reply}");
        assert!(
            geometry_has(&reply, "win-2"),
            "switch keeps retained tiled {reply}"
        );
        assert!(
            !geometry_has(&reply, "win-3"),
            "new one-axis floats {reply}"
        );
        // Step 3, switched back to both-axes with win-3 adopted floating
        // (as the adapter carries it): the automatic float is preserved,
        // never re-tiled by the switch.
        let reply = parse_reply(&planner.evaluate(&request_with(
            "pred-switch-3",
            serde_json::json!([
                {"window": "win-1", "output": "out-1", "workspace": "ws-1", "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
                hints("win-2"),
                floating_hints("win-3"),
            ]),
            Some("both-axes-fixed"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert!(geometry_has(&reply, "win-2"), "{reply}");
        assert!(
            !geometry_has(&reply, "win-3"),
            "automatic float preserved {reply}"
        );
        // Step 4, explicit user tile override wins for the live client.
        let reply = parse_reply(&planner.evaluate(&request_with(
            "pred-switch-4",
            serde_json::json!([
                {"window": "win-1", "output": "out-1", "workspace": "ws-1", "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
                hints("win-2"),
                suppressed_hints("win-3"),
            ]),
            Some("either-axis-fixed"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert!(geometry_has(&reply, "win-3"), "override tiles {reply}");
        // Step 5, the recorded override keeps win-3 tiled without the signal.
        let reply = parse_reply(&planner.evaluate(&request_with(
            "pred-switch-5",
            serde_json::json!([
                {"window": "win-1", "output": "out-1", "workspace": "ws-1", "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
                hints("win-2"),
                hints("win-3"),
            ]),
            Some("either-axis-fixed"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert!(geometry_has(&reply, "win-3"), "override persists {reply}");
    }

    #[test]
    fn drag_drop_edge_plans_geometry_then_refusals_stay_usable() {
        // Synchronous drag-drop: one seeded two-window domain, an edge drop
        // plans full geometry as `drag-drop`/`place-tiled`, while center,
        // outside-work-area, and unfocused drops refuse bounded `rejected`
        // (never terminal) with canonical topology kept and no pending drag:
        // each refusal is followed by a usable request that still plans.
        let seed = || {
            let mut planner = Planner::new();
            for (cid, focused, windows, command) in [
                (
                    "dd-s1",
                    "win-1",
                    vec![("win-1", 0, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
                (
                    "dd-s2",
                    "win-1",
                    vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                    serde_json::json!({"op": "reconcile"}),
                ),
            ] {
                let reply = parse_reply(&planner.evaluate(&retained_request(
                    cid, "owner-1", "gen-1", focused, &windows, command,
                )));
                assert_eq!(reply["outcome"], "planned", "{reply}");
            }
            planner
        };
        let two = vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        // Edge accepted: focused win-1 dropped on the top edge of the
        // projected win-2 tile wraps the column vertically.
        let mut planner = seed();
        let edge = parse_reply(&planner.evaluate(&retained_request(
            "dd-edge-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(edge["outcome"], "planned", "{edge}");
        assert_eq!(edge["detail"]["kind"], "drag-drop", "{edge}");
        assert_eq!(edge["detail"]["capability"], "place-tiled", "{edge}");
        assert_eq!(edge["base_revision"], 2, "{edge}");
        let geometry = edge["desired_geometry"].as_array().expect("geometry");
        assert_eq!(geometry.len(), 2, "{edge}");
        let mut rects: Vec<(String, i32, i32, i32, i32)> = geometry
            .iter()
            .map(|g| {
                (
                    g["window"].as_str().unwrap_or_default().to_owned(),
                    g["rect"]["x"].as_i64().unwrap_or(-1) as i32,
                    g["rect"]["y"].as_i64().unwrap_or(-1) as i32,
                    g["rect"]["w"].as_i64().unwrap_or(-1) as i32,
                    g["rect"]["h"].as_i64().unwrap_or(-1) as i32,
                )
            })
            .collect();
        rects.sort();
        assert_eq!(
            rects,
            vec![
                ("win-1".to_owned(), 0, 0, 1200, 400),
                ("win-2".to_owned(), 0, 400, 1200, 400),
            ],
            "{edge}",
        );
        assert_eq!(edge["desired_focus"]["leaf"], "leaf-win-1", "{edge}");

        // Center (stack fact) refuses as bounded rejected.
        let mut planner = seed();
        let center = parse_reply(&planner.evaluate(&retained_request(
            "dd-center-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 400}),
        )));
        assert_eq!(center["outcome"], "rejected", "{center}");
        assert_eq!(center["kind"], "unsupported-capability", "{center}");
        // Still usable: the same edge drop plans afterwards.
        let after_center = parse_reply(&planner.evaluate(&retained_request(
            "dd-center-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(after_center["outcome"], "planned", "{after_center}");
        assert_eq!(
            after_center["detail"]["kind"], "drag-drop",
            "{after_center}"
        );

        // Outside the work area refuses as bounded rejected (snap-back).
        let mut planner = seed();
        let outside = parse_reply(&planner.evaluate(&retained_request(
            "dd-out-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 2000, "y": 2000}),
        )));
        assert_eq!(outside["outcome"], "rejected", "{outside}");
        // A plain reconcile still projects the retained pair afterwards.
        let after_outside = parse_reply(&planner.evaluate(&retained_request(
            "dd-out-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(after_outside["outcome"], "planned", "{after_outside}");
        assert_eq!(
            after_outside["desired_geometry"].as_array().map(Vec::len),
            Some(2),
            "{after_outside}"
        );

        // Unfocused dragged window refuses as bounded rejected.
        let mut planner = seed();
        let unfocused = parse_reply(&planner.evaluate(&retained_request(
            "dd-unf-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-drop", "window": "win-2", "x": 5, "y": 400}),
        )));
        assert_eq!(unfocused["outcome"], "rejected", "{unfocused}");
        assert_eq!(unfocused["kind"], "focus-mismatch", "{unfocused}");
        // Still usable: the focused edge drop plans afterwards.
        let after_unfocused = parse_reply(&planner.evaluate(&retained_request(
            "dd-unf-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(after_unfocused["outcome"], "planned", "{after_unfocused}");
    }

    #[test]
    fn drag_drop_op_shape_refuses_fail_closed() {
        // Wire-shape fences stay in protocol at their exact positions: a
        // present-but-wrong op, an opaque-id violation, and unknown fields
        // refuse without touching retained state.
        let mut planner = Planner::new();
        let wrong_op = parse_reply(&planner.evaluate(&retained_request(
            "dd-shape-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 1}),
        )));
        assert_eq!(wrong_op["outcome"], "rejected", "{wrong_op}");
        assert_eq!(wrong_op["kind"], "request-malformed", "{wrong_op}");
        let bad_window = parse_reply(&planner.evaluate(&retained_request(
            "dd-shape-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "drag-drop", "window": "", "x": 10, "y": 10}),
        )));
        assert_eq!(bad_window["outcome"], "rejected", "{bad_window}");
        assert_eq!(bad_window["kind"], "snapshot-invalid", "{bad_window}");
        assert_eq!(
            bad_window["detail"], "drag-drop-window-invalid",
            "{bad_window}"
        );
        let extra = parse_reply(&planner.evaluate(&retained_request(
            "dd-shape-3",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 10, "y": 10, "bogus": 1}),
        )));
        assert_eq!(extra["outcome"], "rejected", "{extra}");
        assert_eq!(extra["kind"], "unknown-field", "{extra}");
        assert_eq!(planner.retained_domains(), 0);
    }

    #[test]
    fn drag_preview_is_read_only_and_matches_drop_geometry() {
        // Read-only preview over the DescribePlan boundary: same complete
        // observation as drag-drop, proposed rect plus carried hover state,
        // no commit, no revision change, no seeding. The follow-up drop at
        // the same point plans the same target, and a follow-up reconcile
        // projects the untouched retained pair.
        let two = vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        let mut planner = seed_two_window_planner();
        let preview = parse_reply(&planner.evaluate(&retained_request(
            "pv-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(preview["outcome"], "preview", "{preview}");
        assert_eq!(preview["kind"], "drag-preview", "{preview}");
        assert_eq!(preview["base_revision"], 2, "{preview}");
        assert_eq!(preview["detail"]["kind"], "drag-preview", "{preview}");
        assert_eq!(preview["detail"]["capability"], "place-tiled", "{preview}");
        let rect = &preview["preview_rect"];
        assert_eq!(
            rect,
            &serde_json::json!({"x": 0, "y": 0, "w": 1200, "h": 400}),
            "{preview}"
        );
        let hover = &preview["hover_prior"];
        assert_eq!(hover["domain_output"], "out-1", "{preview}");
        assert_eq!(hover["domain_workspace"], "ws-1", "{preview}");
        assert_eq!(hover["source_leaf"], "leaf-win-1", "{preview}");
        assert_eq!(hover["source_window"], "win-1", "{preview}");
        assert_eq!(hover["revision"], 2, "{preview}");
        assert_eq!(planner.retained_domains(), 1);
        // Read-only: a reconcile afterwards still projects the retained pair.
        let after = parse_reply(&planner.evaluate(&retained_request(
            "pv-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(after["outcome"], "planned", "{after}");
        assert_eq!(after["detail"]["kind"], "reconcile", "{after}");
        assert_geometry_covers(&after, &["win-1", "win-2"]);
        // The follow-up drop at the same point plans the same target slot.
        let drop = parse_reply(&planner.evaluate(&retained_request(
            "pv-3",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(drop["outcome"], "planned", "{drop}");
        assert_eq!(drop["detail"]["kind"], "drag-drop", "{drop}");
        assert_eq!(
            drop["base_revision"], preview["base_revision"],
            "{drop} vs {preview}: preview must not advance the revision"
        );
        let geometry = drop["desired_geometry"].as_array().expect("geometry");
        let moved = geometry
            .iter()
            .find(|g| g["window"] == "win-1")
            .expect("moved geometry");
        assert_eq!(
            moved["rect"],
            serde_json::json!({"x": 0, "y": 0, "w": 1200, "h": 400}),
            "{drop} vs {preview}"
        );
    }

    #[test]
    fn drag_preview_op_shape_and_size_hints_match_drop() {
        // Wire-shape fences mirror drag-drop (wrong op, bad window, unknown
        // fields, unknown prior fields). Fresh size hints shape the proposed
        // rect exactly like the drop's final geometry.
        let mut planner = Planner::new();
        let wrong_op = parse_reply(&planner.evaluate(&retained_request(
            "pv-shape-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 1}),
        )));
        assert_eq!(wrong_op["outcome"], "rejected", "{wrong_op}");
        assert_eq!(wrong_op["kind"], "request-malformed", "{wrong_op}");
        let extra = parse_reply(&planner.evaluate(&retained_request(
            "pv-shape-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 10, "y": 10, "bogus": 1}),
        )));
        assert_eq!(extra["outcome"], "rejected", "{extra}");
        assert_eq!(extra["kind"], "unknown-field", "{extra}");
        let bad_prior = parse_reply(&planner.evaluate(&retained_request(
            "pv-shape-3",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 10, "y": 10, "hover_prior": {"bogus": 1}}),
        )));
        assert_eq!(bad_prior["outcome"], "rejected", "{bad_prior}");
        assert_eq!(bad_prior["kind"], "unknown-field", "{bad_prior}");
        // Size hints: seed a pair, then preview/drop with a min_w hint on the
        // sibling. Both must agree on the narrowed proposed rect.
        let mut planner = seed_two_window_planner();
        let two = vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        let mut hints = std::collections::BTreeMap::new();
        hints.insert("win-2", (Some((800, 100)), None));
        let preview = parse_reply(&planner.evaluate(&retained_request_with_hints(
            "pv-hint-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            &hints,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(preview["outcome"], "preview", "{preview}");
        let drop = parse_reply(&planner.evaluate(&retained_request_with_hints(
            "pv-hint-2",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            &hints,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(drop["outcome"], "planned", "{drop}");
        let moved = drop["desired_geometry"]
            .as_array()
            .expect("geometry")
            .iter()
            .find(|g| g["window"] == "win-1")
            .expect("moved geometry");
        assert_eq!(
            moved["rect"], preview["preview_rect"],
            "{drop} vs {preview}"
        );
    }

    #[test]
    fn drag_hover_prior_carries_sticky_edge_and_invalid_prior_is_ignored() {
        // The preview's hover_prior forwards verbatim into the next preview
        // and the final drop. A semantically invalid carry (wrong revision)
        // is ignored advisory-style: the request still resolves without
        // enabling arbitrary hover.
        let mut planner = seed_two_window_planner();
        let two = vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        let first = parse_reply(&planner.evaluate(&retained_request(
            "hp-1",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(first["outcome"], "preview", "{first}");
        let hover = first["hover_prior"].clone();
        assert_eq!(hover["source_window"], "win-1", "{first}");
        // Forward the exact carry into the next preview: still resolves with
        // the same proposed rect.
        let mut command =
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5});
        command["hover_prior"] = hover.clone();
        let second = parse_reply(&planner.evaluate(&retained_request(
            "hp-2", "owner-1", "gen-1", "win-1", &two, command,
        )));
        assert_eq!(second["outcome"], "preview", "{second}");
        assert_eq!(
            second["preview_rect"], first["preview_rect"],
            "{second} vs {first}"
        );
        // Invalid carry (wrong revision) is ignored: still previews the same
        // slot. Checked before any committing drop so the retained topology
        // still matches the preview point.
        let mut bad = hover.clone();
        bad["revision"] = serde_json::json!(999_999);
        let mut bad_command =
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5});
        bad_command["hover_prior"] = bad.clone();
        let bad_preview = parse_reply(&planner.evaluate(&retained_request(
            "hp-4",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            bad_command,
        )));
        assert_eq!(bad_preview["outcome"], "preview", "{bad_preview}");
        assert_eq!(
            bad_preview["preview_rect"], first["preview_rect"],
            "{bad_preview} vs {first}"
        );
        // Forward the exact carry into the final drop: plans the same slot.
        let mut drop_command =
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5});
        drop_command["hover_prior"] = hover.clone();
        let drop = parse_reply(&planner.evaluate(&retained_request(
            "hp-3",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            drop_command,
        )));
        assert_eq!(drop["outcome"], "planned", "{drop}");
        let moved = drop["desired_geometry"]
            .as_array()
            .expect("geometry")
            .iter()
            .find(|g| g["window"] == "win-1")
            .expect("moved geometry");
        assert_eq!(moved["rect"], first["preview_rect"], "{drop} vs {first}");
        // Invalid prior edge token is also ignored, never arbitrary.
        let mut bad_edge = hover.clone();
        bad_edge["prior"] = serde_json::json!({"group": "grp-x", "edge": "center"});
        let mut bad_edge_command =
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5});
        bad_edge_command["hover_prior"] = bad_edge;
        // Fresh planner so the drop commits from the seeded revision.
        let mut fresh = seed_two_window_planner();
        let bad_drop = parse_reply(&fresh.evaluate(&retained_request(
            "hp-5",
            "owner-1",
            "gen-1",
            "win-1",
            &two,
            bad_edge_command,
        )));
        assert_eq!(bad_drop["outcome"], "planned", "{bad_drop}");
    }
    #[test]
    fn drag_singleton_source_binding_gates_cross_output_admit() {
        // Singleton snap-back stays `unchanged` without a source binding, with
        // a same-output binding, or with an invalid binding. Only an explicit
        // validated Started source on a different output admits, for both
        // drop and preview. Invalid bindings degrade advisory-style (no
        // refusal of the shape, just no singleton admit).
        let seed_singleton = || {
            let mut planner = Planner::new();
            let reply = parse_reply(&planner.evaluate(&retained_request(
                "sg-seed-1",
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
            planner
        };
        let one = vec![("win-1", 0, 0, 100, 80)];
        // Same-output drop (no source) snaps back.
        let mut planner = seed_singleton();
        let same = parse_reply(&planner.evaluate(&retained_request(
            "sg-drop-same",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 600, "y": 400}),
        )));
        assert_eq!(same["outcome"], "rejected", "{same}");
        assert_eq!(same["kind"], "unchanged", "{same}");
        // Same-output binding also snaps back (no fabricated target).
        let same_bound = parse_reply(&planner.evaluate(&retained_request(
            "sg-drop-same-bound",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 600, "y": 400, "source_output": "out-1", "source_workspace": "ws-1"}),
        )));
        assert_eq!(same_bound["outcome"], "rejected", "{same_bound}");
        assert_eq!(same_bound["kind"], "unchanged", "{same_bound}");
        // Invalid binding degrades to same-output (no admit, no shape refusal).
        let invalid = parse_reply(&planner.evaluate(&retained_request(
            "sg-drop-invalid",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 600, "y": 400, "source_output": "!!!", "source_workspace": "ws-1"}),
        )));
        assert_eq!(invalid["outcome"], "rejected", "{invalid}");
        assert_eq!(invalid["kind"], "unchanged", "{invalid}");
        // Cross-output binding admits the singleton placement.
        let cross = parse_reply(&planner.evaluate(&retained_request(
            "sg-drop-cross",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 600, "y": 400, "source_output": "out-2", "source_workspace": "ws-1"}),
        )));
        assert_eq!(cross["outcome"], "planned", "{cross}");
        assert_eq!(cross["detail"]["kind"], "drag-drop", "{cross}");
        // Preview mirrors drop: same-output snaps back, cross-output previews.
        let mut preview_planner = seed_singleton();
        let pv_same = parse_reply(&preview_planner.evaluate(&retained_request(
            "sg-pv-same",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 600, "y": 400}),
        )));
        assert_eq!(pv_same["outcome"], "rejected", "{pv_same}");
        assert_eq!(pv_same["kind"], "unchanged", "{pv_same}");
        let pv_cross = parse_reply(&preview_planner.evaluate(&retained_request(
            "sg-pv-cross",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 600, "y": 400, "source_output": "out-2", "source_workspace": "ws-1"}),
        )));
        assert_eq!(pv_cross["outcome"], "preview", "{pv_cross}");
        assert_eq!(pv_cross["kind"], "drag-preview", "{pv_cross}");
    }
    #[test]
    fn drag_observed_out_of_bounds_frames_converge() {
        // Observed frame rectangles are host drift: any valid carried rect
        // is accepted and converges downstream, while invalid rects refuse.
        let straddling = vec![("win-1", 1100, 0, 200, 80), ("win-2", 200, 0, 100, 80)];
        let mut planner = seed_two_window_planner();
        let preview = parse_reply(&planner.evaluate(&retained_request(
            "drag-straddle-pv",
            "owner-1",
            "gen-1",
            "win-1",
            &straddling,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(preview["outcome"], "preview", "{preview}");
        assert_eq!(preview["kind"], "drag-preview", "{preview}");
        let drop = parse_reply(&planner.evaluate(&retained_request(
            "drag-straddle-drop",
            "owner-1",
            "gen-1",
            "win-1",
            &straddling,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(drop["outcome"], "planned", "{drop}");
        assert_eq!(drop["detail"]["kind"], "drag-drop", "{drop}");
        // Fully outside-but-valid source rect is also accepted while the
        // pointer stays inside the work area.
        let mut planner = seed_two_window_planner();
        let outside_source = vec![("win-1", 1300, 100, 100, 80), ("win-2", 200, 0, 100, 80)];
        let preview_out = parse_reply(&planner.evaluate(&retained_request(
            "drag-outsider-src-pv",
            "owner-1",
            "gen-1",
            "win-1",
            &outside_source,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(preview_out["outcome"], "preview", "{preview_out}");
        // Out-of-bounds survivors are host drift too: drag ops and
        // reconcile accept them and converge downstream.
        let outsider_survivor = vec![("win-1", 0, 0, 100, 80), ("win-2", 1300, 0, 100, 80)];
        let mut planner = seed_two_window_planner();
        let survivor_pv = parse_reply(&planner.evaluate(&retained_request(
            "drag-survivor-pv",
            "owner-1",
            "gen-1",
            "win-1",
            &outsider_survivor,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(survivor_pv["outcome"], "preview", "{survivor_pv}");
        let survivor_drop = parse_reply(&planner.evaluate(&retained_request(
            "drag-survivor-drop",
            "owner-1",
            "gen-1",
            "win-1",
            &outsider_survivor,
            serde_json::json!({"op": "drag-drop", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(survivor_drop["outcome"], "planned", "{survivor_drop}");
        let survivor_rec = parse_reply(&planner.evaluate(&retained_request(
            "drag-survivor-rec",
            "owner-1",
            "gen-1",
            "win-1",
            &outsider_survivor,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(survivor_rec["outcome"], "planned", "{survivor_rec}");
        assert_geometry_covers(&survivor_rec, &["win-1", "win-2"]);
        // Invalid moved-source rects still fence, even for drag ops.
        let mut planner = seed_two_window_planner();
        let invalid_source = vec![("win-1", 0, 0, 0, 80), ("win-2", 200, 0, 100, 80)];
        let invalid = parse_reply(&planner.evaluate(&retained_request(
            "drag-invalid-src-pv",
            "owner-1",
            "gen-1",
            "win-1",
            &invalid_source,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        )));
        assert_eq!(invalid["outcome"], "rejected", "{invalid}");
        assert_eq!(invalid["kind"], "snapshot-invalid", "{invalid}");
        assert_eq!(invalid["detail"], "window-rect-invalid", "{invalid}");
    }
    #[test]
    fn drag_preview_stage_summary_shape_is_bounded() {
        // Log-only diagnostic shape: correlated closed stage token, the
        // suppressed count, and the suppressed span; no identifiers,
        // geometry, domains, owner, or payload bytes.
        assert_eq!(
            summarize_drag_preview_stage("corr-stage-1", "resolve-outside", 0, "-", "-"),
            "omnitiler:plan-summary direction=drag-stage op=drag-preview correlation=corr-stage-1 stage=resolve-outside suppressed=0 span_first=- span_last=-",
        );
        assert_eq!(
            summarize_drag_preview_stage(
                "corr-stage-2",
                "pair-converge",
                7,
                "corr-stage-0",
                "corr-stage-1"
            ),
            "omnitiler:plan-summary direction=drag-stage op=drag-preview correlation=corr-stage-2 stage=pair-converge suppressed=7 span_first=corr-stage-0 span_last=corr-stage-1",
        );
        // Unparseable sides degrade to bounded placeholders like the other
        // summaries.
        assert_eq!(
            summarize_drag_preview_stage("!!!", "entry-homing", 0, "!!!", "???"),
            "omnitiler:plan-summary direction=drag-stage op=drag-preview correlation=- stage=entry-homing suppressed=0 span_first=- span_last=-",
        );
        assert_eq!(
            summarize_drag_preview_stage("corr-stage-3", "Bogus Stage!", 0, "-", "-"),
            "omnitiler:plan-summary direction=drag-stage op=drag-preview correlation=corr-stage-3 stage=unknown suppressed=0 span_first=- span_last=-",
        );
    }
    #[test]
    fn preview_stage_dedup_coalesces_identical_stages() {
        let mut gate = super::PreviewStageDedup::default();
        let first = gate.observe("corr-s-1", "resolve-outside");
        assert_eq!(first.len(), 1, "first stage of a run emits, {first:?}");
        assert!(first[0].contains("stage=resolve-outside"), "{first:?}");
        assert!(first[0].contains("suppressed=0"), "{first:?}");
        for seq in 2..6 {
            let lines = gate.observe(&format!("corr-s-{seq}"), "resolve-outside");
            assert!(
                lines.is_empty(),
                "identical stage repeats stay silent, {lines:?}"
            );
        }
        let lines = gate.observe("corr-s-6", "converge");
        assert_eq!(
            lines.len(),
            2,
            "transition flushes the run count, {lines:?}"
        );
        assert!(
            lines[0].contains("stage=resolve-outside")
                && lines[0].contains("suppressed=4")
                && lines[0].contains("span_first=corr-s-2")
                && lines[0].contains("span_last=corr-s-5"),
            "flush keeps the suppressed span, {lines:?}"
        );
        assert!(
            lines[1].contains("stage=converge") && lines[1].contains("suppressed=0"),
            "{lines:?}"
        );
    }
    #[test]
    fn preview_stage_dedup_flushes_on_reset() {
        let mut gate = super::PreviewStageDedup::default();
        assert!(gate.reset("corr-r-0").is_empty(), "idle reset stays silent");
        assert_eq!(gate.observe("corr-r-1", "begin").len(), 1);
        assert!(gate.observe("corr-r-2", "begin").is_empty());
        assert!(gate.observe("corr-r-3", "begin").is_empty());
        let lines = gate.reset("corr-r-4");
        assert_eq!(lines.len(), 1, "reset flushes the pending count, {lines:?}");
        assert!(
            lines[0].contains("stage=begin")
                && lines[0].contains("suppressed=2")
                && lines[0].contains("span_first=corr-r-2")
                && lines[0].contains("span_last=corr-r-3"),
            "{lines:?}"
        );
        assert!(
            gate.reset("corr-r-5").is_empty(),
            "second reset stays silent"
        );
    }
    #[test]
    fn drag_preview_entry_homing_keeps_wire_bytes() {
        // A carried row homed outside the request domain refuses at entry
        // validation with byte-identical wire behavior (rejected,
        // cross-domain-mismatch, no detail field, egress detail `-`); the
        // new stage diagnostic is log-only.
        let one = vec![("win-1", 0, 0, 100, 80)];
        let mut request: serde_json::Value = serde_json::from_str(&retained_request(
            "drag-entry-homing-1",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "drag-preview", "window": "win-1", "x": 900, "y": 5}),
        ))
        .expect("request JSON");
        request["windows"][0]["output"] = serde_json::json!("out-9");
        let request_text = request.to_string();
        let reply_text = Planner::new().evaluate(&request_text);
        let reply = parse_reply(&reply_text);
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "cross-domain-mismatch", "{reply}");
        assert!(
            reply.get("detail").is_none(),
            "entry refusal carries no detail field, got {reply}"
        );
        assert_eq!(
            summarize_plan_egress(&request_text, &reply_text),
            "omnitiler:plan-summary direction=egress op=drag-preview correlation=drag-entry-homing-1 outcome=rejected kind=cross-domain-mismatch base_revision=- detail=-",
        );
    }
    #[test]
    fn untracked_floating_toggle_float_rect_precedence_and_fresh_unfloat() {
        // Probe precedence covers an invalid `float_rect` too: an untracked
        // floating window on an absent session reports `not-tiled`, never
        // `float-rect-invalid`. A valid rect (or none) reaches the Engine,
        // which converges the fresh floating observation into an exception
        // and unfloats it into the current domain.
        let one = vec![("win-1", 0, 0, 100, 80)];
        let mut bad_rect: serde_json::Value = serde_json::from_str(&retained_request(
            "gold-float-rect-1",
            "owner-1",
            "gen-1",
            "win-1",
            &one,
            serde_json::json!({"op": "toggle-float", "window": "win-1", "float_rect": {"x": 0, "y": 0, "w": 0, "h": 80}}),
        ))
        .expect("request JSON");
        bad_rect["windows"][0]["floating"] = serde_json::Value::Bool(true);
        assert_eq!(
            Planner::new().evaluate(&bad_rect.to_string()),
            "{\"v\":1,\"correlation_id\":\"gold-float-rect-1\",\"outcome\":\"rejected\",\"kind\":\"not-tiled\",\"message\":\"focused window is not a tiled window\"}",
        );
        for (correlation, command) in [
            (
                "gold-float-rect-2",
                serde_json::json!({"op": "toggle-float", "window": "win-1"}),
            ),
            (
                "gold-float-rect-3",
                serde_json::json!({"op": "toggle-float", "window": "win-1", "float_rect": {"x": 240, "y": 160, "w": 720, "h": 480}}),
            ),
        ] {
            let mut request: serde_json::Value = serde_json::from_str(&retained_request(
                correlation,
                "owner-1",
                "gen-1",
                "win-1",
                &one,
                command,
            ))
            .expect("request JSON");
            request["windows"][0]["floating"] = serde_json::Value::Bool(true);
            let reply = parse_reply(&Planner::new().evaluate(&request.to_string()));
            assert_eq!(reply["outcome"], "planned", "{reply}");
            assert_eq!(reply["float_geometry"], serde_json::Value::Null, "{reply}");
            let geometry = reply["desired_geometry"].as_array().expect("geometry");
            assert_eq!(geometry.len(), 1, "{reply}");
            assert_eq!(geometry[0]["window"], "win-1", "{reply}");
            assert_eq!(geometry[0]["workspace"], "ws-1", "{reply}");
        }
    }
    #[test]
    fn planner_snapshot_detail_registry_is_unique() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for token in PLANNER_SNAPSHOT_DETAILS {
            assert!(!token.is_empty(), "empty token");
            assert!(token.len() <= 32, "token too long: {token}");
            assert!(
                token
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "token must be short lowercase-hyphenated ASCII: {token}"
            );
            assert!(
                !token.contains("window-count-mismatch"),
                "must not generalize existing producer"
            );
            assert!(seen.insert(*token), "duplicate token: {token}");
        }
        assert_eq!(PLANNER_SNAPSHOT_DETAILS.len(), 54, "closed registry size");
    }

    fn geometry_by_window(
        reply: &serde_json::Value,
    ) -> std::collections::BTreeMap<String, (i32, i32, i32, i32)> {
        reply["desired_geometry"]
            .as_array()
            .expect("planned geometry present")
            .iter()
            .map(|entry| {
                let rect = &entry["rect"];
                (
                    entry["window"].as_str().expect("window").to_owned(),
                    (
                        rect["x"].as_i64().unwrap() as i32,
                        rect["y"].as_i64().unwrap() as i32,
                        rect["w"].as_i64().unwrap() as i32,
                        rect["h"].as_i64().unwrap() as i32,
                    ),
                )
            })
            .collect()
    }

    fn seed_two_window_planner() -> Planner {
        let mut planner = Planner::new();
        for (correlation, focused, windows, command) in [
            (
                "rec-seed-1",
                "win-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "rec-seed-2",
                "win-1",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let request =
                retained_request(correlation, "owner-1", "gen-1", focused, &windows, command);
            let reply = parse_reply(&planner.evaluate(&request));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        assert_eq!(planner.retained_domains(), 1);
        planner
    }

    #[test]
    fn reconcile_retains_allocation_despite_changed_observed_rects() {
        let mut planner = seed_two_window_planner();
        let baseline = retained_request(
            "rec-base-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let baseline_reply = parse_reply(&planner.evaluate(&baseline));
        assert_eq!(baseline_reply["outcome"], "planned", "{baseline_reply}");
        assert_eq!(
            baseline_reply["detail"]["kind"], "reconcile",
            "{baseline_reply}"
        );
        assert_geometry_covers(&baseline_reply, &["win-1", "win-2"]);
        let before = geometry_by_window(&baseline_reply);
        // Same membership with deliberately drifted client rectangles; the
        // authoritative allocation must not move and shares/topology must not
        // change. Observed rects stay within domain bounds so only allocation
        // retention is exercised, never snapshot validation.
        let drifted = retained_request(
            "rec-drift-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 10, 10), ("win-2", 1100, 700, 50, 50)],
            serde_json::json!({"op": "reconcile"}),
        );
        let drifted_reply = parse_reply(&planner.evaluate(&drifted));
        assert_eq!(drifted_reply["outcome"], "planned", "{drifted_reply}");
        assert_eq!(
            drifted_reply["detail"]["kind"], "reconcile",
            "{drifted_reply}"
        );
        assert_geometry_covers(&drifted_reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_by_window(&drifted_reply),
            before,
            "{drifted_reply} vs {baseline_reply}"
        );
        assert_eq!(planner.retained_domains(), 1);
        // No pending crosses calls and no topology rebuild: a follow-up
        // directional command still plans on the retained tree.
        let follow = retained_request(
            "rec-follow-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "focus", "window": "win-1", "direction": "right"}),
        );
        let follow_reply = parse_reply(&planner.evaluate(&follow));
        assert_eq!(follow_reply["outcome"], "planned", "{follow_reply}");
    }

    #[test]
    fn reconcile_fresh_horizontal_fit_projects_exact_geometry() {
        // Fresh adoption returns the complete projection without separate
        // admit requests.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "fresh-fit-rec",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 600, 800)),
                ("win-2".to_owned(), (600, 0, 600, 800)),
            ]),
            "{reply}"
        );
        assert_eq!(planner.retained_domains(), 1);
        assert_eq!(
            planner.engine.last_adoption_fit().unwrap().outcome,
            "fitted"
        );
    }

    #[test]
    fn reconcile_fresh_fallback_seeds_deterministic_geometry() {
        // Identical centres on both axes have no cut: fallback to the normal
        // deterministic seed with zero centre splits.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "fresh-seed-rec",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 1200, 800), ("win-2", 0, 0, 1200, 800)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 600, 800)),
                ("win-2".to_owned(), (600, 0, 600, 800)),
            ]),
            "{reply}"
        );
        let report = planner.engine.last_adoption_fit().expect("fallback logs");
        assert_eq!(report.outcome, "fallback", "{reply}");
        assert_eq!(report.reason, "no_cut", "{reply}");
        assert_eq!(report.centre_splits, 0, "{reply}");
        assert_eq!(planner.retained_domains(), 1);
    }

    #[test]
    fn reconcile_fresh_mixed_converges_exception() {
        // Focused tiled newcomer plus a floating member: the tiled set
        // plans while the floating member converges as an exception.
        let mut value: serde_json::Value = serde_json::from_str(&retained_request(
            "fresh-mixed-rec",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("valid retained request");
        value["windows"][1]["floating"] = serde_json::json!(true);
        value["windows"][1]["fit_excluded"] = serde_json::json!(true);
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1"]);
        assert_eq!(planner.retained_domains(), 1);
    }

    #[test]
    fn reconcile_fresh_all_floating_projects_empty() {
        let mut value: serde_json::Value = serde_json::from_str(&retained_request(
            "fresh-float-rec",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("valid retained request");
        for entry in value["windows"].as_array_mut().expect("windows") {
            entry["floating"] = serde_json::json!(true);
            entry["fit_excluded"] = serde_json::json!(true);
        }
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(0),
            "{reply}"
        );
        // Floating exceptions retain the slot without tiled geometry.
        assert_eq!(planner.retained_domains(), 1);
    }

    #[test]
    fn reconcile_fresh_empty_projects_empty_without_session() {
        let mut value: serde_json::Value = serde_json::from_str(&retained_request(
            "fresh-empty-rec",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("valid retained request");
        value["windows"] = serde_json::json!([]);
        value["focused_window"] = serde_json::json!("");
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(0),
            "{reply}"
        );
        assert_eq!(planner.retained_domains(), 0);
    }

    #[test]
    fn reconcile_fresh_relocation_uses_retained_source() {
        // Same-workspace displaced source relocates on a fresh reconcile
        // carrying the complete moved observation instead of seeding anew.
        let mut planner = seed_two_window_planner();
        let moved = retained_request_for_domain(
            "fresh-reloc-rec",
            "owner-1",
            "gen-1",
            "out-2",
            "ws-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        // Entries above are homed to out-1 by the helper; re-home the
        // complete observation to the target output for the moved domain.
        let mut value: serde_json::Value = serde_json::from_str(&moved).expect("valid request");
        for entry in value["windows"].as_array_mut().expect("windows") {
            entry["output"] = serde_json::json!("out-2");
        }
        let reply = parse_reply(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
    }

    #[test]
    fn update_gaps_exception_only_adopts_without_topology_error() {
        // Both members observed floating: convergence drops the tiled tree,
        // and update-gaps adopts the carried gaps while projecting empty
        // instead of `malformed-topology`.
        let mut planner = seed_two_window_planner();
        // Float win-2 first (win-1 still tiles), then both: convergence
        // adopts each exception while the tree drains to nothing.
        for (correlation, float_win_1) in [("gap-float-1", false), ("gap-float-2", true)] {
            let mut value: serde_json::Value = serde_json::from_str(&retained_request(
                correlation,
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ))
            .expect("valid retained request");
            value["windows"][1]["floating"] = serde_json::json!(true);
            value["windows"][1]["fit_excluded"] = serde_json::json!(true);
            if float_win_1 {
                value["windows"][0]["floating"] = serde_json::json!(true);
                value["windows"][0]["fit_excluded"] = serde_json::json!(true);
            }
            let reply = parse_reply(&planner.evaluate(&value.to_string()));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        let mut gaps: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                "gap-exc-1",
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "update-gaps"}),
            ))
            .expect("valid gaps request");
        for entry in gaps["windows"].as_array_mut().expect("windows") {
            entry["floating"] = serde_json::json!(true);
            entry["fit_excluded"] = serde_json::json!(true);
        }
        let reply = parse_reply(&planner.evaluate(&gaps.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(0),
            "{reply}"
        );
        // The adopted gaps bind later fences: old gaps mismatch, new gaps plan.
        let stale = retained_request(
            "gap-exc-stale",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let stale_reply = parse_reply(&planner.evaluate(&stale));
        assert_eq!(stale_reply["outcome"], "rejected", "{stale_reply}");
    }

    #[test]
    fn update_gaps_fresh_tiled_still_unknown_domain() {
        // update-gaps never seeds: a fresh tiled domain refuses so the
        // reconcile seed path owns it.
        let mut planner = Planner::new();
        let request = retained_request(
            "gap-fresh-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "update-gaps"}),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "unknown-domain", "{reply}");
        assert_eq!(planner.retained_domains(), 0);
    }

    #[test]
    fn reconcile_retained_empty_reconcile_retires_slot() {
        // D23 tentative: omission at the converge layer is ambiguous (close
        // vs minimize indistinguishable), so an empty complete observation
        // retains the session with stored minimize slots instead of
        // retiring; explicit removal still retires. Session-level Remove
        // proposals stage no slots (`PendingDesired` carries none).
        let mut planner = seed_two_window_planner();
        let mut value: serde_json::Value = serde_json::from_str(&retained_request(
            "empty-retire-rec",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("valid retained request");
        value["windows"] = serde_json::json!([]);
        value["focused_window"] = serde_json::json!("");
        let reply = parse_reply(&planner.evaluate(&value.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(0),
            "{reply}"
        );
        assert!(reply["desired_focus"].is_null(), "{reply}");
        assert_eq!(planner.retained_domains(), 1);
        // Genuine retire still occurs via explicit removal: release-domain
        // drops the slot with no native writes.
        value["correlation_id"] = serde_json::json!("empty-retire-release");
        value["command"] = serde_json::json!({"op": "release-domain"});
        let release = parse_reply(&planner.evaluate(&value.to_string()));
        assert_eq!(release["outcome"], "released", "{release}");
        assert_eq!(release["kind"], "release-domain", "{release}");
        assert_eq!(planner.retained_domains(), 0);
    }

    #[test]
    fn reconcile_without_hints_carries_no_hint_flags() {
        let mut planner = seed_two_window_planner();
        let request = retained_request(
            "rec-flags-absent",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        for entry in reply["desired_geometry"]
            .as_array()
            .expect("planned geometry present")
        {
            assert!(entry.get("overconstrained").is_none(), "{reply}");
            assert!(entry.get("client_clamped").is_none(), "{reply}");
        }
    }

    #[test]
    fn reconcile_accepts_hinted_short_frame_as_client_clamped() {
        let mut planner = seed_two_window_planner();
        let baseline = retained_request(
            "rec-clamp-base",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let baseline_reply = parse_reply(&planner.evaluate(&baseline));
        assert_eq!(baseline_reply["outcome"], "planned", "{baseline_reply}");
        let before = geometry_by_window(&baseline_reply);
        // Short frame against a REAL carried maximum: win-2 keeps position
        // but lands 56 short in height, exactly at its carried max, so the
        // observed size equals clamp(desired, min, max) and accepts. (This
        // is distinct from the logged Ghostty evidence, whose max is the
        // unbounded sentinel: that stays genuine drift.) Hints ride
        // alongside the legacy fingerprint (7): advisory hints never join
        // the fingerprint.
        let desired_h = before["win-2"].3;
        let clamped_h = desired_h - 56;
        let (x, y, w, _) = before["win-2"];
        let mut hints = std::collections::BTreeMap::new();
        hints.insert("win-2", (None, Some((w, clamped_h))));
        let clamped = retained_request_with_hints(
            "rec-clamp-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", x, y, w, clamped_h)],
            &hints,
            serde_json::json!({"op": "reconcile"}),
        );
        let reply = parse_reply(&planner.evaluate(&clamped));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        // Retained truth stands (leftover explained, shares untouched)...
        assert_eq!(geometry_by_window(&reply), before, "{reply}");
        // ...while the clamped window accepts: no rewrite, no drift/park.
        let by_window: std::collections::BTreeMap<String, &serde_json::Value> =
            reply["desired_geometry"]
                .as_array()
                .expect("planned geometry present")
                .iter()
                .map(|entry| (entry["window"].as_str().expect("window").to_owned(), entry))
                .collect();
        assert_eq!(
            by_window["win-2"].get("client_clamped"),
            Some(&serde_json::Value::Bool(true)),
            "{reply}"
        );
        assert!(
            by_window["win-2"].get("overconstrained").is_none(),
            "{reply}"
        );
        assert!(
            by_window["win-1"].get("client_clamped").is_none(),
            "{reply}"
        );
        assert!(
            by_window["win-1"].get("overconstrained").is_none(),
            "{reply}"
        );
        // No pending crosses the read-only reconcile: a follow-up command
        // still plans on the retained tree.
        let follow = retained_request(
            "rec-clamp-follow",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "focus", "window": "win-1", "direction": "right"}),
        );
        let follow_reply = parse_reply(&planner.evaluate(&follow));
        assert_eq!(follow_reply["outcome"], "planned", "{follow_reply}");
    }

    #[test]
    fn reconcile_marks_unsatisfiable_minimums_overconstrained() {
        let mut planner = seed_two_window_planner();
        // Both windows demand 700 wide in a 1200 extent: unsatisfiable.
        let mut hints = std::collections::BTreeMap::new();
        hints.insert("win-1", (Some((700, 10)), None));
        hints.insert("win-2", (Some((700, 10)), None));
        let request = retained_request_with_hints(
            "rec-over-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            &hints,
            serde_json::json!({"op": "reconcile"}),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        for entry in reply["desired_geometry"]
            .as_array()
            .expect("planned geometry present")
        {
            assert_eq!(
                entry.get("overconstrained"),
                Some(&serde_json::Value::Bool(true)),
                "{reply}"
            );
            assert!(entry.get("client_clamped").is_none(), "{reply}");
        }
    }

    #[test]
    fn reconcile_unhinted_drift_never_accepts() {
        let mut planner = seed_two_window_planner();
        let baseline = retained_request(
            "rec-nodrift-base",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let baseline_reply = parse_reply(&planner.evaluate(&baseline));
        let before = geometry_by_window(&baseline_reply);
        // Same 56-short frame with no hints: drift, still fully reasserted.
        let (x, y, w, h) = before["win-2"];
        let drifted = retained_request(
            "rec-nodrift-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", x, y, w, h - 56)],
            serde_json::json!({"op": "reconcile"}),
        );
        let reply = parse_reply(&planner.evaluate(&drifted));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(geometry_by_window(&reply), before, "{reply}");
        for entry in reply["desired_geometry"]
            .as_array()
            .expect("planned geometry present")
        {
            assert!(entry.get("client_clamped").is_none(), "{reply}");
            assert!(entry.get("overconstrained").is_none(), "{reply}");
        }
    }

    #[test]
    fn reconcile_honors_satisfiable_minimum_from_observed_hints() {
        let mut planner = seed_two_window_planner();
        // A satisfiable observed minimum converges the newcomer through the
        // fresh complete observation; win-3 needs 400 wide.
        let mut hints = std::collections::BTreeMap::new();
        hints.insert("win-3", (Some((400, 10)), None));
        let request: serde_json::Value = serde_json::from_str(&retained_request_with_hints(
            "rec-hint-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 80),
            ],
            &hints,
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("valid request");
        // Converge through the normal path with the hinted observation.
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        let geometry = geometry_by_window(&reply);
        assert_eq!(geometry.len(), 3, "{reply}");
        let win3 = geometry["win-3"];
        assert!(win3.2 >= 400, "converged minimum honored: {reply}");
        for entry in reply["desired_geometry"]
            .as_array()
            .expect("planned geometry present")
        {
            assert!(entry.get("overconstrained").is_none(), "{reply}");
        }
    }

    #[test]
    fn hint_unknown_nested_field_rejects_fail_closed() {
        let mut planner = seed_two_window_planner();
        let mut request: serde_json::Value = serde_json::from_str(&retained_request(
            "rec-hint-bad",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("valid request");
        request["windows"][0]["min_size"] = serde_json::json!({"w": 10, "h": 10, "depth": 3});
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "unknown-field", "{reply}");
    }

    #[test]
    fn discarded_reconcile_reply_leaves_retained_session_ready_for_finish_and_pointer_resize() {
        let mut planner = seed_two_window_planner();
        // The adapter may suppress this reply after native resize starts, but
        // the server has already completed this pure retained projection.
        let held = retained_request(
            "rec-held-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 500, 80), ("win-2", 500, 0, 700, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let held_reply = parse_reply(&planner.evaluate(&held));
        assert_eq!(held_reply["outcome"], "planned", "{held_reply}");

        // A no-oracle finish resync remains usable in the same retained
        // session: reconcile stages no Session pending transaction.
        let finish = retained_request(
            "rec-finish-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 450, 80), ("win-2", 450, 0, 750, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let finish_reply = parse_reply(&planner.evaluate(&finish));
        assert_eq!(finish_reply["outcome"], "planned", "{finish_reply}");
        assert_eq!(
            geometry_by_window(&finish_reply),
            geometry_by_window(&held_reply),
            "finish resync retains the original allocation"
        );

        // A delayed valid oracle verdict can still commit the selected pointer
        // share adjustment after either retained projection has been evaluated.
        let pointer = retained_request(
            "rec-pointer-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({
                "op": "pointer-resize",
                "window": "win-2",
                "direction": "left",
                "boundary": 550,
            }),
        );
        let pointer_reply = parse_reply(&planner.evaluate(&pointer));
        assert_eq!(pointer_reply["outcome"], "planned", "{pointer_reply}");
        assert_eq!(
            pointer_reply["detail"]["kind"], "pointer-resize",
            "{pointer_reply}"
        );
        assert_ne!(
            pointer_reply["detail"]["old_shares"], pointer_reply["detail"]["new_shares"],
            "the delayed oracle boundary remains authoritative"
        );
    }

    #[test]
    fn reconcile_reprojects_retained_tree_for_scale_style_work_area_change() {
        let mut planner = Planner::new();
        for (correlation, windows, command) in [
            (
                "rec-gap-seed-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "rec-gap-seed-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let reply = parse_reply(&planner.evaluate(&retained_request_with_selected_gaps(
                correlation,
                "owner-1",
                "gen-1",
                "win-1",
                &windows,
                command,
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        let mut request: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                "rec-grow-1",
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ))
            .expect("valid request");
        request["domain"]["bounds"] = serde_json::json!({"x": 0, "y": 0, "w": 1800, "h": 1200});
        assert_eq!(request["domain"]["gap"], 8);
        assert_eq!(request["domain"]["outer_gap"], 8);
        let grown = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(grown["outcome"], "planned", "{grown}");
        assert_eq!(
            geometry_by_window(&grown),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (8, 8, 888, 1184)),
                ("win-2".to_owned(), (904, 8, 888, 1184)),
            ])
        );

        request["correlation_id"] = serde_json::json!("rec-shrink-1");
        request["domain"]["bounds"] = serde_json::json!({"x": 0, "y": 0, "w": 800, "h": 600});
        let shrunk = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(shrunk["outcome"], "planned", "{shrunk}");
        assert_eq!(
            geometry_by_window(&shrunk),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (8, 8, 388, 584)),
                ("win-2".to_owned(), (404, 8, 388, 584)),
            ])
        );
    }

    #[test]
    fn reconcile_rejects_changed_outer_gap_as_a_domain_mismatch() {
        let mut planner = Planner::new();
        for (correlation, windows, command) in [
            (
                "rec-outer-seed-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "rec-outer-seed-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let reply = parse_reply(&planner.evaluate(&retained_request_with_selected_gaps(
                correlation,
                "owner-1",
                "gen-1",
                "win-1",
                &windows,
                command,
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        let mut request: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                "rec-outer-reproject-1",
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ))
            .expect("valid request");
        request["domain"]["outer_gap"] = serde_json::json!(0);
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "domain-mismatch", "{reply}");
        assert_eq!(
            reply["message"],
            "domain outer gap does not match retained state"
        );
        assert!(reply.get("detail").is_none(), "{reply}");
    }

    fn seed_gap_planner() -> Planner {
        let mut planner = Planner::new();
        for (correlation, windows, command) in [
            (
                "gap-seed-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "gap-seed-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let reply = parse_reply(&planner.evaluate(&retained_request_with_selected_gaps(
                correlation,
                "owner-1",
                "gen-1",
                "win-1",
                &windows,
                command,
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        assert_eq!(planner.retained_domains(), 1);
        planner
    }

    fn update_gaps_request(correlation: &str, inner: i32, outer: i32) -> String {
        let mut request: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                correlation,
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "update-gaps"}),
            ))
            .expect("valid request");
        request["domain"]["gap"] = serde_json::json!(inner);
        request["domain"]["outer_gap"] = serde_json::json!(outer);
        request.to_string()
    }

    fn reconcile_gaps_request(correlation: &str, inner: i32, outer: i32) -> String {
        let mut request: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                correlation,
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ))
            .expect("valid request");
        request["domain"]["gap"] = serde_json::json!(inner);
        request["domain"]["outer_gap"] = serde_json::json!(outer);
        request.to_string()
    }

    #[test]
    fn update_gaps_accepts_changed_inner_gap_and_reprojects_retained_tree() {
        let mut planner = seed_gap_planner();
        let baseline = parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-base-1", 8, 8)));
        assert_eq!(baseline["outcome"], "planned", "{baseline}");
        assert_eq!(baseline["detail"]["kind"], "reconcile", "{baseline}");
        assert_geometry_covers(&baseline, &["win-1", "win-2"]);
        let before = geometry_by_window(&baseline);
        assert_eq!(
            before,
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (8, 8, 588, 784)),
                ("win-2".to_owned(), (604, 8, 588, 784)),
            ]),
            "{baseline}"
        );

        let updated = parse_reply(&planner.evaluate(&update_gaps_request("gap-inner-1", 16, 8)));
        assert_eq!(updated["outcome"], "planned", "{updated}");
        assert_eq!(updated["detail"]["kind"], "update-gaps", "{updated}");
        assert_eq!(
            updated["detail"]["capability"], "update-gaps-geometry",
            "{updated}"
        );
        assert_geometry_covers(&updated, &["win-1", "win-2"]);
        // Native-apply-relevant reply: the retained tree reprojected with the
        // new inner gap into the unchanged outer-inset bounds.
        assert_eq!(
            geometry_by_window(&updated),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (8, 8, 584, 784)),
                ("win-2".to_owned(), (608, 8, 584, 784)),
            ]),
            "{updated}"
        );
        assert_ne!(geometry_by_window(&updated), before, "{updated}");
        // Focus and accepted revision are preserved, not reseeded.
        assert_eq!(
            updated["desired_focus"], baseline["desired_focus"],
            "{updated}"
        );
        assert_eq!(
            updated["base_revision"], baseline["base_revision"],
            "{updated}"
        );
        assert_eq!(planner.retained_domains(), 1);

        // The retained session now owns the new gaps: a normal reconcile with
        // the new gaps converges, while the old-flight gaps refuse.
        let converged =
            parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-inner-2", 16, 8)));
        assert_eq!(converged["outcome"], "planned", "{converged}");
        assert_eq!(
            geometry_by_window(&converged),
            geometry_by_window(&updated),
            "{converged} vs {updated}"
        );
        let stale = parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-inner-3", 8, 8)));
        assert_eq!(stale["outcome"], "rejected", "{stale}");
        assert_eq!(stale["kind"], "domain-mismatch", "{stale}");

        // Round trip back to the original gaps restores the exact baseline
        // allocation: topology and shares survived, nothing reseeded.
        let restored = parse_reply(&planner.evaluate(&update_gaps_request("gap-inner-4", 8, 8)));
        assert_eq!(restored["outcome"], "planned", "{restored}");
        assert_eq!(geometry_by_window(&restored), before, "{restored}");
        assert_eq!(
            restored["desired_focus"], baseline["desired_focus"],
            "{restored}"
        );
    }

    #[test]
    fn update_gaps_accepts_changed_outer_gap_and_reprojects_retained_tree() {
        let mut planner = seed_gap_planner();
        let baseline = parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-obase-1", 8, 8)));
        assert_eq!(baseline["outcome"], "planned", "{baseline}");
        let before = geometry_by_window(&baseline);

        let updated = parse_reply(&planner.evaluate(&update_gaps_request("gap-outer-1", 8, 0)));
        assert_eq!(updated["outcome"], "planned", "{updated}");
        assert_eq!(updated["detail"]["kind"], "update-gaps", "{updated}");
        assert_geometry_covers(&updated, &["win-1", "win-2"]);
        // Outer zero drops the work-area inset: the full 1200x800 carries the
        // retained split with the unchanged inner gap.
        assert_eq!(
            geometry_by_window(&updated),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 596, 800)),
                ("win-2".to_owned(), (604, 0, 596, 800)),
            ]),
            "{updated}"
        );
        assert_ne!(geometry_by_window(&updated), before, "{updated}");
        assert_eq!(
            updated["desired_focus"], baseline["desired_focus"],
            "{updated}"
        );
        assert_eq!(
            updated["base_revision"], baseline["base_revision"],
            "{updated}"
        );
        assert_eq!(planner.retained_domains(), 1);

        let converged =
            parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-outer-2", 8, 0)));
        assert_eq!(converged["outcome"], "planned", "{converged}");
        assert_eq!(
            geometry_by_window(&converged),
            geometry_by_window(&updated),
            "{converged} vs {updated}"
        );
        let stale = parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-outer-3", 8, 8)));
        assert_eq!(stale["outcome"], "rejected", "{stale}");
        assert_eq!(stale["kind"], "domain-mismatch", "{stale}");

        let restored = parse_reply(&planner.evaluate(&update_gaps_request("gap-outer-4", 8, 8)));
        assert_eq!(restored["outcome"], "planned", "{restored}");
        assert_eq!(geometry_by_window(&restored), before, "{restored}");
    }

    #[test]
    fn update_gaps_accepts_combined_inner_and_outer_change() {
        let mut planner = seed_gap_planner();
        let baseline = parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-cbase-1", 8, 8)));
        assert_eq!(baseline["outcome"], "planned", "{baseline}");

        let updated = parse_reply(&planner.evaluate(&update_gaps_request("gap-combo-1", 4, 12)));
        assert_eq!(updated["outcome"], "planned", "{updated}");
        assert_eq!(updated["detail"]["kind"], "update-gaps", "{updated}");
        assert_geometry_covers(&updated, &["win-1", "win-2"]);
        // Outer 12 insets to (12,12,1176,776); inner 4 splits 1172 into 586s.
        assert_eq!(
            geometry_by_window(&updated),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (12, 12, 586, 776)),
                ("win-2".to_owned(), (602, 12, 586, 776)),
            ]),
            "{updated}"
        );
        assert_eq!(
            updated["desired_focus"], baseline["desired_focus"],
            "{updated}"
        );
        assert_eq!(
            updated["base_revision"], baseline["base_revision"],
            "{updated}"
        );

        // A later directional command still plans on the retained tree with
        // the new gaps instead of reseeding.
        let mut follow: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                "gap-combo-2",
                "owner-1",
                "gen-1",
                "win-1",
                &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "focus", "window": "win-1", "direction": "right"}),
            ))
            .expect("valid request");
        follow["domain"]["gap"] = serde_json::json!(4);
        follow["domain"]["outer_gap"] = serde_json::json!(12);
        let follow_reply = parse_reply(&planner.evaluate(&follow.to_string()));
        assert_eq!(follow_reply["outcome"], "planned", "{follow_reply}");
    }

    #[test]
    fn update_gaps_preserves_resized_shares_across_round_trip() {
        let mut planner = seed_gap_planner();
        let resized = parse_reply(&planner.evaluate(&retained_request_with_selected_gaps(
            "gap-share-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({
                "op": "resize",
                "window": "win-1",
                "direction": "right",
                "mode": "outwards",
                "press_index": 0,
            }),
        )));
        assert_eq!(resized["outcome"], "planned", "{resized}");
        let shares = geometry_by_window(&resized);
        assert_ne!(
            shares["win-1"].2, shares["win-2"].2,
            "resize must leave unequal shares {resized}"
        );
        let baseline = parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-share-2", 8, 8)));
        assert_eq!(baseline["outcome"], "planned", "{baseline}");
        let before = geometry_by_window(&baseline);
        assert_ne!(
            before["win-1"].2, before["win-2"].2,
            "reconcile must retain resized shares {baseline}"
        );

        let updated = parse_reply(&planner.evaluate(&update_gaps_request("gap-share-3", 16, 8)));
        assert_eq!(updated["outcome"], "planned", "{updated}");
        let reprojected = geometry_by_window(&updated);
        assert_ne!(
            reprojected["win-1"].2, reprojected["win-2"].2,
            "gap reprojection must retain resized shares {updated}"
        );
        // Leaf identity is stable: the same retained leaves carry new rects.
        let leaves_before: std::collections::BTreeSet<String> = baseline["desired_geometry"]
            .as_array()
            .expect("geometry")
            .iter()
            .map(|g| g["leaf"].as_str().expect("leaf").to_owned())
            .collect();
        let leaves_after: std::collections::BTreeSet<String> = updated["desired_geometry"]
            .as_array()
            .expect("geometry")
            .iter()
            .map(|g| g["leaf"].as_str().expect("leaf").to_owned())
            .collect();
        assert_eq!(leaves_before, leaves_after, "{updated}");

        let restored = parse_reply(&planner.evaluate(&update_gaps_request("gap-share-4", 8, 8)));
        assert_eq!(restored["outcome"], "planned", "{restored}");
        assert_eq!(
            geometry_by_window(&restored),
            before,
            "round trip must restore resized allocation exactly {restored}"
        );
    }

    #[test]
    fn update_gaps_converges_membership_change_then_projects() {
        let mut planner = seed_gap_planner();
        let baseline = parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-part-1", 8, 8)));
        assert_eq!(baseline["outcome"], "planned", "{baseline}");
        assert_geometry_covers(&baseline, &["win-1", "win-2"]);

        // Complete-observation convergence
        // (docs/changes/archive/observation-convergence.md): the missing retained
        // member is removed before the ordinary gap update projects the
        // survivor with the new gaps. No `partial-observation` remains on
        // this path.
        let mut partial: serde_json::Value =
            serde_json::from_str(&update_gaps_request("gap-part-2", 16, 8)).expect("valid request");
        partial["windows"] = serde_json::json!([
            {"window": "win-1", "output": "out-1", "workspace": "ws-1",
             "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
        ]);
        let converged = parse_reply(&planner.evaluate(&partial.to_string()));
        assert_eq!(converged["outcome"], "planned", "{converged}");
        assert_eq!(converged["detail"]["kind"], "update-gaps", "{converged}");
        assert_geometry_covers(&converged, &["win-1"]);

        // The converged state is retained: an exact observation of the
        // survivor at the adopted gaps still plans.
        let mut exact: serde_json::Value =
            serde_json::from_str(&reconcile_gaps_request("gap-part-3", 16, 8))
                .expect("valid request");
        exact["windows"] = serde_json::json!([
            {"window": "win-1", "output": "out-1", "workspace": "ws-1",
             "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
        ]);
        let replanned = parse_reply(&planner.evaluate(&exact.to_string()));
        assert_eq!(replanned["outcome"], "planned", "{replanned}");
        assert_geometry_covers(&replanned, &["win-1"]);
    }

    #[test]
    fn update_gaps_refuses_unknown_domain_without_seeding() {
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&update_gaps_request("gap-unknown-1", 16, 8)));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "unknown-domain", "{reply}");
        assert_eq!(planner.retained_domains(), 0);
    }

    #[test]
    fn update_gaps_rejects_out_of_range_gaps_as_snapshot_invalid() {
        let mut planner = seed_gap_planner();
        for (correlation, inner, outer, expected) in [
            ("gap-range-1", -1, 8, "gap-low"),
            ("gap-range-2", 65, 8, "gap-high"),
            ("gap-range-3", 8, -1, "outer-gap-low"),
            ("gap-range-4", 8, 65, "outer-gap-high"),
        ] {
            let reply =
                parse_reply(&planner.evaluate(&update_gaps_request(correlation, inner, outer)));
            assert_eq!(reply["outcome"], "rejected", "{reply}");
            assert_eq!(reply["kind"], "snapshot-invalid", "{reply}");
            assert_eq!(reply["detail"], expected, "{reply}");
        }
        // The refused range checks mutated nothing: old gaps still converge.
        let converged =
            parse_reply(&planner.evaluate(&reconcile_gaps_request("gap-range-5", 8, 8)));
        assert_eq!(converged["outcome"], "planned", "{converged}");
    }

    #[test]
    fn update_gaps_rejects_malformed_command_shape() {
        let mut planner = seed_gap_planner();
        let mut request: serde_json::Value =
            serde_json::from_str(&update_gaps_request("gap-shape-1", 16, 8))
                .expect("valid request");
        request["command"] = serde_json::json!({"op": "update-gaps", "extra": 1});
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "unknown-field", "{reply}");
    }

    #[test]
    fn reconcile_reprojects_unequal_nested_retained_shares_not_observed_rects() {
        let mut planner = Planner::new();
        for (correlation, focused, windows) in [
            ("rec-nested-seed-1", "win-1", vec![("win-1", 0, 0, 100, 80)]),
            (
                "rec-nested-seed-2",
                "win-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            ),
            (
                "rec-nested-seed-3",
                "win-3",
                vec![
                    ("win-1", 0, 0, 100, 80),
                    ("win-2", 200, 0, 100, 80),
                    ("win-3", 400, 0, 100, 80),
                ],
            ),
        ] {
            let reply = parse_reply(&planner.evaluate(&retained_request_with_selected_gaps(
                correlation,
                "owner-1",
                "gen-1",
                focused,
                &windows,
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        let resized = parse_reply(&planner.evaluate(&retained_request_with_selected_gaps(
            "rec-nested-resize",
            "owner-1",
            "gen-1",
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 80),
            ],
            serde_json::json!({
                "op": "resize",
                "window": "win-1",
                "direction": "right",
                "mode": "outwards",
                "press_index": 0,
            }),
        )));
        assert_eq!(resized["outcome"], "planned", "{resized}");
        let before = geometry_by_window(&resized);
        assert_ne!(
            before["win-1"].2, before["win-3"].2,
            "resize must retain unequal nested shares"
        );

        let mut request: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                "rec-nested-reproject",
                "owner-1",
                "gen-1",
                "win-1",
                &[
                    ("win-1", 100, 50, 100, 100),
                    ("win-2", 500, 200, 200, 200),
                    ("win-3", 700, 400, 100, 100),
                ],
                serde_json::json!({"op": "reconcile"}),
            ))
            .expect("valid request");
        request["domain"]["bounds"] = serde_json::json!({"x": 100, "y": 50, "w": 1000, "h": 700});
        let reprojected = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reprojected["outcome"], "planned", "{reprojected}");
        let after = geometry_by_window(&reprojected);
        assert_ne!(
            after["win-1"].2, after["win-3"].2,
            "nested retained shares must survive reprojection"
        );
        assert_ne!(
            after["win-1"],
            (108, 58, 100, 100),
            "reprojection must not adopt observed client rectangles"
        );
    }

    #[test]
    fn reconcile_membership_change_converges_before_projecting() {
        // Complete-observation convergence (docs/changes/archive/observation-convergence.md):
        // a missing retained member is removed and an unexpected new normal
        // member is admitted through normal placement before the ordinary
        // reconcile projects the converged survivors. No single-reason
        // `partial-observation` remains on this path.
        let mut planner = seed_two_window_planner();
        let mismatched = retained_request(
            "rec-mismatch-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-3", 400, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let reply = parse_reply(&planner.evaluate(&mismatched));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["detail"]["kind"], "reconcile", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-3"]);
        assert_ne!(reply["kind"], "diverged", "{reply}");
        // Converged state is retained: the next complete observation of the
        // converged set still reconciles.
        assert_eq!(planner.retained_domains(), 1);
        let recover = retained_request(
            "rec-mismatch-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-3", 400, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let recovered = parse_reply(&planner.evaluate(&recover));
        assert_eq!(recovered["outcome"], "planned", "{recovered}");
        assert_geometry_covers(&recovered, &["win-1", "win-3"]);
    }

    #[test]
    fn reconcile_converges_floating_skew_without_partial_observation() {
        // Retained tiled win-1 observed floating: convergence removes its
        // leaf, retains a floating exception, and the reconcile projects the
        // survivors without `partial-observation` or reset.
        let mut planner = seed_two_window_planner();
        let mut skewed: serde_json::Value = serde_json::from_str(&retained_request(
            "rec-float-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("valid request");
        skewed["windows"][0]["floating"] = serde_json::json!(true);
        let reply = parse_reply(&planner.evaluate(&skewed.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["detail"]["kind"], "reconcile", "{reply}");
        assert_geometry_covers(&reply, &["win-2"]);
    }

    #[test]
    fn reconcile_with_post_removal_observation_projects_survivor() {
        // KWin sends the current post-removal observation: win-2 already
        // departed, so complete-observation reconcile converges it away and
        // projects the survivor (reconcile capability, valid focus).
        let mut planner = Planner::new();
        let seed = parse_reply(&planner.evaluate(&retained_request(
            "post-rem-rec-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(seed["outcome"], "planned", "{seed}");
        assert_eq!(planner.retained_domains(), 1);
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "post-rem-rec-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["detail"]["kind"], "reconcile", "{reply}");
        assert_geometry_covers(&reply, &["win-1"]);
        assert_eq!(planner.retained_domains(), 1);
    }

    #[test]
    fn exact_reconcile_keeps_revision_without_convergence() {
        // Exact-match observation: no convergence bump. Two identical
        // reconciles report the same base revision; the projection is a pure
        // function of retained state, not a mutation.
        let mut planner = seed_two_window_planner();
        let windows = &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        let first = parse_reply(&planner.evaluate(&retained_request(
            "rec-exact-1",
            "owner-1",
            "gen-1",
            "win-1",
            windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(first["outcome"], "planned", "{first}");
        let second = parse_reply(&planner.evaluate(&retained_request(
            "rec-exact-2",
            "owner-1",
            "gen-1",
            "win-1",
            windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(second["outcome"], "planned", "{second}");
        assert_eq!(second["base_revision"], first["base_revision"], "{second}");
        assert_eq!(
            geometry_by_window(&second),
            geometry_by_window(&first),
            "{second}"
        );
    }

    #[test]
    fn convergence_summaries_are_bounded_and_redacted() {
        let line = summarize_plan_convergence("conv-sum-1", "reconcile", 1, 2, 3);
        assert_eq!(
            line,
            "omnitiler:plan-summary direction=convergence op=reconcile correlation=conv-sum-1 reason=observation-mismatch removed=1 admitted=2 flags_adopted=3"
        );
        // Malformed sides degrade to bounded placeholders without echoing
        // anything caller-controlled: no window ids, rects, owner, or
        // payload bytes.
        let garbage = summarize_plan_convergence("evil correlation!!", "Reconcile!!", 0, 0, 0);
        assert_eq!(
            garbage,
            "omnitiler:plan-summary direction=convergence op=unknown correlation=- reason=observation-mismatch removed=0 admitted=0 flags_adopted=0"
        );
        assert!(!garbage.contains("evil"), "{garbage}");
    }

    #[test]
    fn invalid_command_paths_have_single_precise_reason() {
        let mut planner = seed_two_window_planner();
        // Unknown op stays coarse-free with exactly one kind.
        let unknown = retained_request(
            "inv-unknown-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "bogus-op"}),
        );
        let unknown_reply = parse_reply(&planner.evaluate(&unknown));
        assert_eq!(unknown_reply["outcome"], "rejected", "{unknown_reply}");
        assert_eq!(unknown_reply["kind"], "unknown-value", "{unknown_reply}");
        // Strict shape: extra fields reject without rebuilding.
        let extra = retained_request(
            "inv-extra-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile", "extra": 1}),
        );
        let extra_reply = parse_reply(&planner.evaluate(&extra));
        assert_eq!(extra_reply["outcome"], "rejected", "{extra_reply}");
        assert_eq!(extra_reply["kind"], "unknown-field", "{extra_reply}");
        // Direct op-mismatch details are exact single tokens.
        let cid = "inv-op-1";
        let mut ctx = validate_request(&retained_request(
            cid,
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("base valid");
        ctx.request.command["op"] = serde_json::json!("bogus-op");
        let text = planner.evaluate_reconcile_retained(&ctx);
        let reply = parse_reply(&text);
        assert_eq!(reply["kind"], "snapshot-invalid", "{reply}");
        assert_eq!(reply["detail"], "reconcile-op-invalid", "{reply}");
    }

    fn workspace_entry(window: &str, workspace: &str, x: i32) -> serde_json::Value {
        serde_json::json!({
            "window": window,
            "output": "out-1",
            "workspace": workspace,
            "rect": {"x": x, "y": 0, "w": 100, "h": 80},
        })
    }

    /// Full workspace-send request over source ws-1 and target ws-2. `focused`
    /// names the observed focused window (empty when the source is empty) and
    /// `revision` is the carried observation revision (0 on request, the
    /// seeded base on ack/verify). Optional overrides mutate the request
    /// before serialization so refusal routes share one builder.
    #[allow(clippy::too_many_arguments)]
    fn workspace_request(
        correlation: &str,
        owner: &str,
        generation: &str,
        revision: u64,
        focused: &str,
        source: Vec<serde_json::Value>,
        target: Vec<serde_json::Value>,
        command: serde_json::Value,
    ) -> String {
        let windows = if source.is_empty() {
            serde_json::json!([])
        } else {
            serde_json::Value::Array(source)
        };
        let target_windows = serde_json::Value::Array(target);
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": owner,
            "generation": generation,
            "revision": revision,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "target_domain": {
                "output": "out-1",
                "workspace": "ws-2",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": windows,
            "target_windows": target_windows,
            "command": command,
        })
        .to_string()
    }

    fn workspace_send_body() -> serde_json::Value {
        serde_json::json!({
            "op": "send-to-workspace",
            "window": "win-1",
            "target_output": "out-1",
            "target_workspace": "ws-2",
        })
    }

    fn workspace_follow_body(window: &str) -> serde_json::Value {
        serde_json::json!({
            "op": "send-to-workspace",
            "window": window,
            "target_output": "out-1",
            "target_workspace": "ws-2",
            "follow": true,
        })
    }

    fn workspace_stay_body(window: &str) -> serde_json::Value {
        serde_json::json!({
            "op": "send-to-workspace",
            "window": window,
            "target_output": "out-1",
            "target_workspace": "ws-2",
            "follow": false,
        })
    }

    #[test]
    fn workspace_send_defaults_to_follow_without_field() {
        // Legacy requests omit `follow`: the default stays follow, focusing
        // the mover in the target with an explicit follow=true echo.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&workspace_request(
            "ws-follow-default-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_send_body(),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-workspace", "{reply}");
        assert_eq!(reply["operation"]["follow"], true, "{reply}");
        assert_eq!(
            reply["desired_focus"]["domain_workspace"], "ws-2",
            "{reply}"
        );
        // Explicit follow=true plans identically to the omitted default.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&workspace_request(
            "ws-follow-explicit-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_follow_body("win-1"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["operation"]["follow"], true, "{reply}");
        assert_eq!(
            reply["desired_focus"]["domain_workspace"], "ws-2",
            "{reply}"
        );
    }

    #[test]
    fn workspace_stay_keeps_source_focus_without_selecting_target() {
        // Stay moves the focused window but leaves the source selected: the
        // source MRU survivor keeps focus and the operation echoes
        // follow=false. Destination admission matches the follow path. Two
        // reconciles establish the source MRU (win-1, then win-2) so the
        // stay fallback has existing removal-equivalent history.
        let mut planner = Planner::new();
        let both = &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        for (index, focused) in ["win-1", "win-2"].iter().enumerate() {
            let rec = parse_reply(&planner.evaluate(&retained_request_for_domain(
                &format!("ws-stay-focus-{index}"),
                "owner-1",
                "gen-1",
                "out-1",
                "ws-1",
                focused,
                both,
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(rec["outcome"], "planned", "{rec}");
        }
        let reply = parse_reply(&planner.evaluate(&workspace_request(
            "ws-stay-1",
            "owner-1",
            "gen-1",
            0,
            "win-2",
            vec![
                workspace_entry("win-1", "ws-1", 0),
                workspace_entry("win-2", "ws-1", 10),
            ],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_stay_body("win-2"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-workspace", "{reply}");
        assert_eq!(reply["operation"]["follow"], false, "{reply}");
        assert_eq!(reply["operation"]["window"], "win-2", "{reply}");
        assert_eq!(
            reply["desired_focus"]["domain_workspace"], "ws-1",
            "{reply}"
        );
        // The focused leaf is the MRU survivor, never the mover: resolve it
        // through the source geometry entry.
        let focus_leaf = reply["desired_focus"]["leaf"].as_str().expect("focus leaf");
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        let focus_window = geometry
            .iter()
            .find(|g| g["leaf"] == focus_leaf && g["workspace"] == "ws-1")
            .and_then(|g| g["window"].as_str())
            .expect("source focus resolves");
        assert_eq!(focus_window, "win-1", "{reply}");
        let mut members: Vec<(String, String)> = geometry
            .iter()
            .map(|g| {
                (
                    g["window"].as_str().expect("window").to_owned(),
                    g["workspace"].as_str().expect("workspace").to_owned(),
                )
            })
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec![
                ("win-1".to_owned(), "ws-1".to_owned()),
                ("win-2".to_owned(), "ws-2".to_owned()),
                ("win-t1".to_owned(), "ws-2".to_owned()),
            ],
            "{reply}"
        );
    }

    #[test]
    fn workspace_stay_of_sole_source_window_leaves_no_focus() {
        // Stay sending the only source window empties the source slot: no
        // focus remains, the mover lands on the trailing-empty target, and
        // the source carries no geometry.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&workspace_request(
            "ws-stay-empty-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![],
            workspace_stay_body("win-1"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["operation"]["follow"], false, "{reply}");
        assert_eq!(reply["desired_focus"], serde_json::Value::Null, "{reply}");
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        assert_eq!(geometry.len(), 1, "{reply}");
        assert_eq!(geometry[0]["window"], "win-1", "{reply}");
        assert_eq!(geometry[0]["workspace"], "ws-2", "{reply}");
    }

    fn output_entry(window: &str, output: &str, x: i32) -> serde_json::Value {
        serde_json::json!({
            "window": window,
            "output": output,
            "workspace": "ws-1",
            "rect": {"x": x, "y": 0, "w": 100, "h": 80},
        })
    }

    /// Full explicit output-send request over source out-1/ws-1 and the
    /// destination output's current workspace out-2/ws-1 (resolved
    /// adapter-side). Mirrors [`workspace_request`] with a cross-output
    /// target scope.
    fn output_send_request(
        correlation: &str,
        focused: &str,
        source: Vec<serde_json::Value>,
        target: Vec<serde_json::Value>,
        command: serde_json::Value,
    ) -> String {
        let windows = if source.is_empty() {
            serde_json::json!([])
        } else {
            serde_json::Value::Array(source)
        };
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": "owner-1",
            "generation": "gen-1",
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "target_domain": {
                "output": "out-2",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": windows,
            "target_windows": serde_json::Value::Array(target),
            "command": command,
        })
        .to_string()
    }

    fn output_send_body(window: &str) -> serde_json::Value {
        // Legacy shape omits `follow`: the default stays follow.
        serde_json::json!({
            "op": "send-to-output",
            "window": window,
            "target_output": "out-2",
            "target_workspace": "ws-1",
        })
    }

    fn output_stay_body(window: &str) -> serde_json::Value {
        serde_json::json!({
            "op": "send-to-output",
            "window": window,
            "target_output": "out-2",
            "target_workspace": "ws-1",
            "follow": false,
        })
    }

    #[test]
    fn output_send_follow_defaults_with_distinct_wire_kind() {
        // Explicit output send is DISTINCT from workspace send: omitted
        // `follow` defaults true, the reply kind is `send-to-output`, and
        // the `move-tiled` operation carries the cross-output assignment
        // with both-domain geometry.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&output_send_request(
            "out-send-follow-1",
            "win-1",
            vec![output_entry("win-1", "out-1", 0)],
            vec![output_entry("win-t1", "out-2", 0)],
            output_send_body("win-1"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-output", "{reply}");
        assert_eq!(reply["detail"]["kind"], "send-to-output", "{reply}");
        assert_eq!(reply["detail"]["capability"], "move-tiled", "{reply}");
        assert_eq!(reply["operation"]["op"], "move-tiled", "{reply}");
        assert_eq!(reply["operation"]["follow"], true, "{reply}");
        assert_eq!(reply["operation"]["source_output"], "out-1", "{reply}");
        assert_eq!(reply["operation"]["target_output"], "out-2", "{reply}");
        assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
        assert_eq!(
            reply["desired_focus"]["domain_workspace"], "ws-1",
            "{reply}"
        );
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        // The sole source window moved, so the emptied source carries no
        // geometry; the target carries the mover beside the existing window.
        assert_eq!(geometry.len(), 2, "{reply}");
        let mut members: Vec<(&str, &str)> = geometry
            .iter()
            .map(|g| {
                (
                    g["window"].as_str().expect("window"),
                    g["output"].as_str().expect("output"),
                )
            })
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec![("win-1", "out-2"), ("win-t1", "out-2")],
            "{reply}"
        );
    }

    #[test]
    fn output_send_stay_keeps_source_without_selecting_target() {
        // Stay mirrors workspace-send stay across outputs: the source MRU
        // survivor keeps focus, follow=false echoes, and the mover still
        // lands on the destination output. Two reconciles establish the
        // source MRU (win-1, then win-2).
        let mut planner = Planner::new();
        let both = &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)];
        for (index, focused) in ["win-1", "win-2"].iter().enumerate() {
            let rec = parse_reply(&planner.evaluate(&retained_request_for_domain(
                &format!("out-send-stay-focus-{index}"),
                "owner-1",
                "gen-1",
                "out-1",
                "ws-1",
                focused,
                both,
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(rec["outcome"], "planned", "{rec}");
        }
        let reply = parse_reply(&planner.evaluate(&output_send_request(
            "out-send-stay-1",
            "win-2",
            vec![
                output_entry("win-1", "out-1", 0),
                output_entry("win-2", "out-1", 200),
            ],
            vec![output_entry("win-t1", "out-2", 0)],
            output_stay_body("win-2"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-output", "{reply}");
        assert_eq!(reply["operation"]["follow"], false, "{reply}");
        assert_eq!(reply["operation"]["window"], "win-2", "{reply}");
        assert_eq!(reply["desired_focus"]["domain_output"], "out-1", "{reply}");
        let focus_leaf = reply["desired_focus"]["leaf"].as_str().expect("focus leaf");
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        let focus_window = geometry
            .iter()
            .find(|g| g["leaf"] == focus_leaf && g["output"] == "out-1")
            .and_then(|g| g["window"].as_str())
            .expect("source focus resolves");
        assert_eq!(focus_window, "win-1", "{reply}");
        let mut members: Vec<(String, String, String)> = geometry
            .iter()
            .map(|g| {
                (
                    g["window"].as_str().expect("window").to_owned(),
                    g["output"].as_str().expect("output").to_owned(),
                    g["workspace"].as_str().expect("workspace").to_owned(),
                )
            })
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec![
                ("win-1".to_owned(), "out-1".to_owned(), "ws-1".to_owned()),
                ("win-2".to_owned(), "out-2".to_owned(), "ws-1".to_owned()),
                ("win-t1".to_owned(), "out-2".to_owned(), "ws-1".to_owned()),
            ],
            "{reply}"
        );
    }

    #[test]
    fn output_send_mru_fallback_plans_beside_history() {
        // 5.4 focus-history fallback over the wire: the destination's
        // remembered leaf (t2) departs via workspace send first, leaving a
        // stale anchor while the destination history retains t1b. The later
        // output send still plans end to end with the mover on the target
        // and target focus; the beside-history landing itself is pinned
        // structurally at the session/engine layers.
        let mut planner = Planner::new();
        for (index, (focused, windows)) in [
            ("t1a", vec![("t1a", 0, 0, 100, 80)]),
            (
                "t1b",
                vec![("t1a", 0, 0, 100, 80), ("t1b", 200, 0, 100, 80)],
            ),
            (
                "t2",
                vec![
                    ("t1a", 0, 0, 100, 80),
                    ("t1b", 200, 0, 100, 80),
                    ("t2", 400, 0, 100, 80),
                ],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let rec = parse_reply(&planner.evaluate(&retained_request_for_domain(
                &format!("out-send-mru-tgt-{index}"),
                "owner-1",
                "gen-1",
                "out-2",
                "ws-1",
                focused,
                &windows,
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(rec["outcome"], "planned", "{rec}");
        }
        // Depart t2 to a same-output spare workspace, staling the anchor.
        let depart = serde_json::json!({
            "v": 1,
            "correlation_id": "out-send-mru-depart",
            "owner": "owner-1",
            "generation": "gen-1",
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": "out-2",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "target_domain": {
                "output": "out-2",
                "workspace": "ws-2",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": "t2",
            "windows": [
                {"window": "t1a", "output": "out-2", "workspace": "ws-1",
                 "rect": {"x": 0, "y": 0, "w": 100, "h": 80}},
                {"window": "t1b", "output": "out-2", "workspace": "ws-1",
                 "rect": {"x": 200, "y": 0, "w": 100, "h": 80}},
                {"window": "t2", "output": "out-2", "workspace": "ws-1",
                 "rect": {"x": 400, "y": 0, "w": 100, "h": 80}},
            ],
            "target_windows": [],
            "command": {
                "op": "send-to-workspace",
                "window": "t2",
                "target_output": "out-2",
                "target_workspace": "ws-2",
            },
        })
        .to_string();
        let departed = parse_reply(&planner.evaluate(&depart));
        assert_eq!(departed["outcome"], "planned", "{departed}");
        for (index, focused) in ["s1", "s2"].iter().enumerate() {
            let windows: Vec<(&str, i32, i32, i32, i32)> = if *focused == "s1" {
                vec![("s1", 0, 0, 100, 80)]
            } else {
                vec![("s1", 0, 0, 100, 80), ("s2", 200, 0, 100, 80)]
            };
            let rec = parse_reply(&planner.evaluate(&retained_request_for_domain(
                &format!("out-send-mru-src-{index}"),
                "owner-1",
                "gen-1",
                "out-1",
                "ws-1",
                focused,
                &windows,
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(rec["outcome"], "planned", "{rec}");
        }
        let reply = parse_reply(&planner.evaluate(&output_send_request(
            "out-send-mru-1",
            "s2",
            vec![
                output_entry("s1", "out-1", 0),
                output_entry("s2", "out-1", 200),
            ],
            vec![
                output_entry("t1a", "out-2", 0),
                output_entry("t1b", "out-2", 200),
            ],
            output_send_body("s2"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-output", "{reply}");
        assert_eq!(reply["operation"]["follow"], true, "{reply}");
        assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
        let mut members: Vec<(String, String)> = reply["desired_geometry"]
            .as_array()
            .expect("geometry")
            .iter()
            .map(|g| {
                (
                    g["window"].as_str().expect("window").to_owned(),
                    g["output"].as_str().expect("output").to_owned(),
                )
            })
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec![
                ("s1".to_owned(), "out-1".to_owned()),
                ("s2".to_owned(), "out-2".to_owned()),
                ("t1a".to_owned(), "out-2".to_owned()),
                ("t1b".to_owned(), "out-2".to_owned()),
            ],
            "{reply}"
        );
    }

    #[test]
    fn output_send_empty_destination_admits_lone_root() {
        // Genuine root fallback over the wire: the destination output was
        // never observed, so the mover becomes its lone root. Follow focuses
        // the target; stay with a sole source window carries no focus and the
        // reply geometry holds only the mover.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&output_send_request(
            "out-send-empty-follow-1",
            "win-1",
            vec![output_entry("win-1", "out-1", 0)],
            vec![],
            output_send_body("win-1"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-output", "{reply}");
        assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        assert_eq!(geometry.len(), 1, "{reply}");
        assert_eq!(geometry[0]["window"], "win-1", "{reply}");
        assert_eq!(geometry[0]["output"], "out-2", "{reply}");

        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&output_send_request(
            "out-send-empty-stay-1",
            "win-1",
            vec![output_entry("win-1", "out-1", 0)],
            vec![],
            output_stay_body("win-1"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["operation"]["follow"], false, "{reply}");
        assert_eq!(reply["desired_focus"], serde_json::Value::Null, "{reply}");
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        assert_eq!(geometry.len(), 1, "{reply}");
        assert_eq!(geometry[0]["window"], "win-1", "{reply}");
        assert_eq!(geometry[0]["output"], "out-2", "{reply}");
    }

    #[test]
    fn output_send_refuses_same_output_target() {
        // Same-output targets refuse: that scope belongs to workspace send,
        // keeping the two ops distinct.
        let mut planner = Planner::new();
        let mut request: serde_json::Value = serde_json::from_str(&output_send_request(
            "out-send-same-1",
            "win-1",
            vec![output_entry("win-1", "out-1", 0)],
            vec![output_entry("win-t1", "out-2", 0)],
            output_send_body("win-1"),
        ))
        .expect("json");
        request["target_domain"]["output"] = serde_json::json!("out-1");
        request["target_domain"]["workspace"] = serde_json::json!("ws-2");
        request["command"]["target_output"] = serde_json::json!("out-1");
        request["command"]["target_workspace"] = serde_json::json!("ws-2");
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "cross-domain-mismatch", "{reply}");
    }

    #[test]
    fn output_send_refuses_target_and_focus_mismatch() {
        let mut planner = Planner::new();
        let mut request: serde_json::Value = serde_json::from_str(&output_send_request(
            "out-send-mismatch-1",
            "win-1",
            vec![output_entry("win-1", "out-1", 0)],
            vec![output_entry("win-t1", "out-2", 0)],
            output_send_body("win-1"),
        ))
        .expect("json");
        request["command"]["target_workspace"] = serde_json::json!("ws-9");
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "target-mismatch", "{reply}");

        let mut planner = Planner::new();
        let request: serde_json::Value = serde_json::from_str(&output_send_request(
            "out-send-mismatch-2",
            "win-2",
            vec![
                output_entry("win-1", "out-1", 0),
                output_entry("win-2", "out-1", 200),
            ],
            vec![output_entry("win-t1", "out-2", 0)],
            output_send_body("win-1"),
        ))
        .expect("json");
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "focus-mismatch", "{reply}");
    }

    fn migrate_entry(window: &str, floating: bool) -> serde_json::Value {
        let mut entry = serde_json::json!({
            "window": window,
            "output": "out-1",
            "workspace": "ws-1",
            "rect": {"x": 0, "y": 0, "w": 100, "h": 80},
        });
        if floating {
            entry["floating"] = serde_json::Value::Bool(true);
            entry["fit_excluded"] = serde_json::Value::Bool(true);
        }
        entry
    }

    fn migrate_request(
        correlation: &str,
        focused: &str,
        windows: Vec<serde_json::Value>,
        target_output: &str,
        target_workspace: &str,
        command: serde_json::Value,
    ) -> String {
        migrate_request_at_revision(
            correlation,
            0,
            focused,
            windows,
            target_output,
            target_workspace,
            command,
        )
    }

    fn migrate_request_at_revision(
        correlation: &str,
        revision: u64,
        focused: &str,
        windows: Vec<serde_json::Value>,
        target_output: &str,
        target_workspace: &str,
        command: serde_json::Value,
    ) -> String {
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": "owner-1",
            "generation": "gen-1",
            "revision": revision,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "target_domain": {
                "output": target_output,
                "workspace": target_workspace,
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": serde_json::Value::Array(windows),
            "target_windows": [],
            "command": command,
        })
        .to_string()
    }

    fn migrate_body(direction: &str) -> serde_json::Value {
        serde_json::json!({"op": "migrate-workspace", "direction": direction})
    }

    #[test]
    fn migrate_workspace_preserves_floats_and_rekeys() {
        // Seed two tiled members, float one, then migrate right: the
        // workspace keeps its backing id on out-2 with the float exception
        // preserved, the moved tiled client stays focused, and the source
        // slot is gone (a repeat refuses as unknown-domain).
        let mut planner = Planner::new();
        let seed = plan_request(
            "migrate-seed-1",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&seed))["outcome"], "planned");
        let float = plan_request(
            "migrate-float-1",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-float", "window": "win-2"}),
        );
        let floated = parse_reply(&planner.evaluate(&float));
        assert_eq!(floated["outcome"], "planned", "{floated}");
        assert_eq!(floated["float_geometry"]["window"], "win-2", "{floated}");
        let base = floated["base_revision"].as_u64().expect("base revision");
        let reply = parse_reply(&planner.evaluate(&migrate_request_at_revision(
            "migrate-1",
            base,
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", true)],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "migrate-workspace", "{reply}");
        assert_eq!(reply["detail"]["kind"], "migrate-workspace", "{reply}");
        assert_eq!(
            reply["detail"]["capability"], "migrate-workspace",
            "{reply}"
        );
        assert_eq!(
            reply["detail"]["commit"], "retained-rekey-planned",
            "{reply}"
        );
        assert_eq!(reply["detail"]["direction"], "right", "{reply}");
        assert_eq!(reply["detail"]["source_output"], "out-1", "{reply}");
        assert_eq!(reply["detail"]["source_workspace"], "ws-1", "{reply}");
        assert_eq!(reply["detail"]["target_output"], "out-2", "{reply}");
        assert_eq!(reply["detail"]["target_workspace"], "ws-1", "{reply}");
        assert_eq!(reply["detail"]["active_window"], "win-1", "{reply}");
        assert_eq!(reply["detail"]["members"], 2, "{reply}");
        assert_eq!(reply["detail"]["floats"], 1, "{reply}");
        assert_eq!(reply["operation"]["op"], "migrate-workspace", "{reply}");
        assert_eq!(reply["operation"]["direction"], "right", "{reply}");
        assert_eq!(reply["operation"]["source_output"], "out-1", "{reply}");
        assert_eq!(reply["operation"]["target_output"], "out-2", "{reply}");
        assert_eq!(reply["operation"]["active_window"], "win-1", "{reply}");
        let preconditions = reply["preconditions"].as_array().expect("preconditions");
        assert!(
            preconditions
                .iter()
                .any(|p| p == "adapter-must-verify-postconditions"),
            "{reply}"
        );
        // Only the tiled member projects; the float rides as an exception.
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        assert_eq!(geometry.len(), 1, "{reply}");
        assert_eq!(geometry[0]["window"], "win-1", "{reply}");
        assert_eq!(geometry[0]["output"], "out-2", "{reply}");
        assert_eq!(reply["desired_focus"]["domain_output"], "out-2", "{reply}");
        assert_eq!(
            reply["desired_focus"]["domain_workspace"], "ws-1",
            "{reply}"
        );
        // The source slot moved: repeating the same migration refuses on the
        // retained SAME-workspace target residue guard.
        let repeat = parse_reply(&planner.evaluate(&migrate_request(
            "migrate-2",
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", true)],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(repeat["outcome"], "rejected", "{repeat}");
        assert_eq!(repeat["kind"], "unchanged", "{repeat}");
        // Structured correlated log line at the planner boundary.
        assert_eq!(
            summarize_plan_migration("migrate-1", "right", 2, 1, false),
            "omnitiler:plan-summary direction=migration op=migrate-workspace correlation=migrate-1 migrate_direction=right members=2 floats=1 empty=false",
        );
    }

    #[test]
    fn migrate_workspace_refuses_overlay_invalid_and_unknown() {
        let mut planner = Planner::new();
        let seed = plan_request(
            "migrate-neg-seed",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&seed))["outcome"], "planned");
        // D8: fullscreen members carry. Verified on an isolated planner so
        // the refusal checks below keep a pristine source slot.
        {
            let mut carry = Planner::new();
            assert_eq!(parse_reply(&carry.evaluate(&seed))["outcome"], "planned");
            let mut overlay = migrate_entry("win-1", false);
            overlay["fullscreen"] = serde_json::Value::Bool(true);
            overlay["fit_excluded"] = serde_json::Value::Bool(true);
            let carried = parse_reply(&carry.evaluate(&migrate_request(
                "migrate-carry-overlay",
                "win-1",
                vec![overlay, migrate_entry("win-2", false)],
                "out-2",
                "ws-1",
                migrate_body("right"),
            )));
            assert_eq!(carried["outcome"], "planned", "{carried}");
            assert_eq!(carried["kind"], "migrate-workspace", "{carried}");
            assert_eq!(carried["detail"]["active_window"], "win-1", "{carried}");
        }
        // Maximized-shaped fit-excluded (no floating/sticky/fullscreen/maximized) refuses.
        let mut maximized = migrate_entry("win-1", false);
        maximized["fit_excluded"] = serde_json::Value::Bool(true);
        let refused = parse_reply(&planner.evaluate(&migrate_request(
            "migrate-neg-maximized",
            "win-1",
            vec![maximized, migrate_entry("win-2", false)],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "overlay-present", "{refused}");
        // Same-output target refuses (migration is cross-output only).
        let refused = parse_reply(&planner.evaluate(&migrate_request(
            "migrate-neg-same",
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", false)],
            "out-1",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "cross-domain-mismatch", "{refused}");
        // Different-workspace target refuses (stable backing id preserved).
        let refused = parse_reply(&planner.evaluate(&migrate_request(
            "migrate-neg-workspace",
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", false)],
            "out-2",
            "ws-9",
            migrate_body("right"),
        )));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "cross-domain-mismatch", "{refused}");
        // Unknown direction refuses.
        let refused = parse_reply(&planner.evaluate(&migrate_request(
            "migrate-neg-direction",
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", false)],
            "out-2",
            "ws-1",
            migrate_body("sideways"),
        )));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "direction-invalid", "{refused}");
        // Unknown fields refuse fail-closed.
        let refused = parse_reply(&planner.evaluate(&migrate_request(
            "migrate-neg-field",
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", false)],
            "out-2",
            "ws-1",
            serde_json::json!({"op": "migrate-workspace", "direction": "right", "bogus": 1}),
        )));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "unknown-field", "{refused}");
        // Non-empty target observation refuses (SAME-workspace residue guard).
        let mut with_target: serde_json::Value = serde_json::from_str(&migrate_request(
            "migrate-neg-residue",
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", false)],
            "out-2",
            "ws-1",
            migrate_body("right"),
        ))
        .expect("json");
        with_target["target_windows"] = serde_json::json!([{
            "window": "win-x",
            "output": "out-2",
            "workspace": "ws-1",
            "rect": {"x": 0, "y": 0, "w": 100, "h": 80},
        }]);
        let refused = parse_reply(&planner.evaluate(&with_target.to_string()));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "unchanged", "{refused}");
        // Unknown source with members refuses (never autorekeys).
        let mut fresh = Planner::new();
        let refused = parse_reply(&fresh.evaluate(&migrate_request(
            "migrate-neg-unknown",
            "win-z",
            vec![migrate_entry("win-z", false)],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(refused["outcome"], "rejected", "{refused}");
        assert_eq!(refused["kind"], "unknown-domain", "{refused}");
        // A maximized float carries with its exception slot preserved.
        // Verified on an isolated planner to keep the shared refusal
        // planner pristine.
        {
            let mut carry = Planner::new();
            assert_eq!(parse_reply(&carry.evaluate(&seed))["outcome"], "planned");
            let mut max_float = migrate_entry("win-1", true);
            max_float["maximized"] = serde_json::Value::Bool(true);
            let carried = parse_reply(&carry.evaluate(&migrate_request(
                "migrate-carry-max-float",
                "win-1",
                vec![max_float, migrate_entry("win-2", false)],
                "out-2",
                "ws-1",
                migrate_body("right"),
            )));
            assert_eq!(carried["outcome"], "planned", "{carried}");
            assert_eq!(carried["kind"], "migrate-workspace", "{carried}");
            assert_eq!(carried["detail"]["floats"], 1, "{carried}");
        }
    }

    #[test]
    fn migrate_workspace_ignores_wire_revision() {
        // Adapters hardcode revision 0 and every op derives bases
        // internally: an arbitrary revision with exact members still plans.
        let mut planner = Planner::new();
        let seed = plan_request(
            "migrate-ignrev-seed",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "reconcile"}),
        );
        let seeded = parse_reply(&planner.evaluate(&seed));
        assert_eq!(seeded["outcome"], "planned", "{seeded}");
        let base = seeded["base_revision"].as_u64().expect("base revision");
        let reply = parse_reply(&planner.evaluate(&migrate_request_at_revision(
            "migrate-ignrev-1",
            base + 999,
            "win-1",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", false)],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "migrate-workspace", "{reply}");
    }

    #[test]
    fn migrate_workspace_active_float_and_absent_client_reply_shapes() {
        // Active float: no tiled focus is fabricated, but the operation
        // still names the client for native activation after verified
        // arrival. Absent client: members migrate with null active.
        let mut planner = Planner::new();
        let seed = plan_request(
            "migrate-shape-seed",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&seed))["outcome"], "planned");
        let float = plan_request(
            "migrate-shape-float",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "toggle-float", "window": "win-2"}),
        );
        let floated = parse_reply(&planner.evaluate(&float));
        assert_eq!(floated["outcome"], "planned", "{floated}");
        let base = floated["base_revision"].as_u64().expect("base revision");
        let reply = parse_reply(&planner.evaluate(&migrate_request_at_revision(
            "migrate-shape-float-active",
            base,
            "win-2",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", true)],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert!(reply.get("desired_focus").is_none(), "{reply}");
        assert_eq!(reply["operation"]["active_window"], "win-2", "{reply}");
        assert_eq!(reply["detail"]["active_window"], "win-2", "{reply}");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(1),
            "{reply}"
        );
    }

    #[test]
    fn migrate_workspace_absent_client_migrates_without_focus() {
        let mut planner = Planner::new();
        let seed = plan_request(
            "migrate-noactive-seed",
            "win-1",
            &["win-1", "win-2"],
            serde_json::json!({"op": "reconcile"}),
        );
        let seeded = parse_reply(&planner.evaluate(&seed));
        assert_eq!(seeded["outcome"], "planned", "{seeded}");
        let base = seeded["base_revision"].as_u64().expect("base revision");
        let reply = parse_reply(&planner.evaluate(&migrate_request_at_revision(
            "migrate-noactive-1",
            base,
            "",
            vec![migrate_entry("win-1", false), migrate_entry("win-2", false)],
            "out-2",
            "ws-1",
            migrate_body("left"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "migrate-workspace", "{reply}");
        assert!(reply.get("desired_focus").is_none(), "{reply}");
        assert_eq!(
            reply["operation"]["active_window"],
            serde_json::Value::Null,
            "{reply}"
        );
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(2),
            "{reply}"
        );
    }

    #[test]
    fn migrate_workspace_sticky_stays_on_source_wire_proof() {
        // Wire-level proof of the sticky residual: the sticky member takes
        // no target geometry, and a follow-up source reconcile still plans
        // it there while the target residue guard holds.
        let mut planner = Planner::new();
        let seed = plan_request(
            "migrate-sticky-seed",
            "win-1",
            &["win-1", "win-2", "win-s"],
            serde_json::json!({"op": "reconcile"}),
        );
        let seeded = parse_reply(&planner.evaluate(&seed));
        assert_eq!(seeded["outcome"], "planned", "{seeded}");
        let base = seeded["base_revision"].as_u64().expect("base revision");
        let mut sticky = migrate_entry("win-s", false);
        sticky["sticky"] = serde_json::Value::Bool(true);
        sticky["fit_excluded"] = serde_json::Value::Bool(true);
        let reply = parse_reply(&planner.evaluate(&migrate_request_at_revision(
            "migrate-sticky-1",
            base,
            "win-1",
            vec![
                migrate_entry("win-1", false),
                migrate_entry("win-2", false),
                sticky,
            ],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        assert_eq!(geometry.len(), 2, "{reply}");
        assert!(
            geometry.iter().all(|entry| entry["window"] != "win-s"),
            "{reply}"
        );
        assert_eq!(reply["operation"]["active_window"], "win-1", "{reply}");
        // The sticky residual still lives on the source output.
        let mut regather: serde_json::Value = serde_json::from_str(&plan_request(
            "migrate-sticky-regather",
            "win-s",
            &["win-s"],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("json");
        regather["windows"][0]["sticky"] = serde_json::Value::Bool(true);
        regather["windows"][0]["fit_excluded"] = serde_json::Value::Bool(true);
        let regathered = parse_reply(&planner.evaluate(&regather.to_string()));
        assert_eq!(regathered["outcome"], "planned", "{regathered}");
    }

    #[test]
    fn migrate_workspace_sticky_active_echoes_null() {
        // Follow-only on the wire too: a stayed sticky active echoes null
        // focus and null active client, never a source window to activate.
        let mut planner = Planner::new();
        let seed = plan_request(
            "migrate-sactive-seed",
            "win-1",
            &["win-1", "win-s"],
            serde_json::json!({"op": "reconcile"}),
        );
        let seeded = parse_reply(&planner.evaluate(&seed));
        assert_eq!(seeded["outcome"], "planned", "{seeded}");
        let base = seeded["base_revision"].as_u64().expect("base revision");
        let mut sticky = migrate_entry("win-s", false);
        sticky["sticky"] = serde_json::Value::Bool(true);
        sticky["fit_excluded"] = serde_json::Value::Bool(true);
        let reply = parse_reply(&planner.evaluate(&migrate_request_at_revision(
            "migrate-sactive-1",
            base,
            "win-s",
            vec![migrate_entry("win-1", false), sticky],
            "out-2",
            "ws-1",
            migrate_body("right"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert!(reply.get("desired_focus").is_none(), "{reply}");
        assert_eq!(
            reply["operation"]["active_window"],
            serde_json::Value::Null,
            "{reply}"
        );
        assert_eq!(
            reply["detail"]["active_window"],
            serde_json::Value::Null,
            "{reply}"
        );
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(1),
            "{reply}"
        );
    }

    #[test]
    fn migrate_workspace_allows_empty_without_fabricated_focus() {
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&migrate_request(
            "migrate-empty-1",
            "",
            vec![],
            "out-2",
            "ws-1",
            migrate_body("left"),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "migrate-workspace", "{reply}");
        assert_eq!(
            reply["desired_geometry"].as_array().map(Vec::len),
            Some(0),
            "{reply}"
        );
        assert!(reply.get("desired_focus").is_none(), "{reply}");
        assert_eq!(
            summarize_plan_migration("migrate-empty-1", "left", 0, 0, true),
            "omnitiler:plan-summary direction=migration op=migrate-workspace correlation=migrate-empty-1 migrate_direction=left members=0 floats=0 empty=true",
        );
    }

    #[test]
    fn directional_vertical_pair_crosses_after_exhaustion() {
        // Item 5.1/5.2: a sole root leaf crosses stacked outputs Down through
        // the two-domain route with up/down adjacency (FULL output rectangles
        // resolved adapter-side). Seeds mirror the horizontal edge test, then
        // win-1 (sole leaf on DP-6) moves down onto occupied HDMI-2.
        fn reconcile_for(
            correlation: &str,
            output: &str,
            bounds: serde_json::Value,
            focused: &str,
            windows: serde_json::Value,
        ) -> String {
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": 7,
                "domain": {
                    "output": output,
                    "workspace": "ws-1",
                    "bounds": bounds,
                    "gap": 8,
                    "outer_gap": 8,
                },
                "focused_window": focused,
                "windows": windows,
                "command": {"op": "reconcile"},
            })
            .to_string()
        }
        fn stacked_for(correlation: &str, focused: &str, command: serde_json::Value) -> String {
            let domains = serde_json::json!([
                {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 0, "w": 1920, "h": 540},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": {"down": "HDMI-2"},
                },
                {
                    "output": "HDMI-2",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 540, "w": 1920, "h": 540},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": {"up": "DP-6"},
                },
            ]);
            let windows = serde_json::json!([
                {
                    "window": "win-1",
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "rect": {"x": 0, "y": 0, "w": 1920, "h": 540},
                },
                {
                    "window": "win-2",
                    "output": "HDMI-2",
                    "workspace": "ws-1",
                    "rect": {"x": 0, "y": 540, "w": 1920, "h": 540},
                },
            ]);
            let domain_entries: Vec<DirectionalDomainDto> =
                serde_json::from_value(domains.clone()).expect("domains decode");
            let window_entries: Vec<ObservedDto> =
                serde_json::from_value(windows.clone()).expect("windows decode");
            let fingerprint = directional_fingerprint(&domain_entries, focused, &window_entries);
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": fingerprint,
                "domain": {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 0, "w": 1920, "h": 540},
                    "gap": 8,
                    "outer_gap": 8,
                },
                "domains": domains,
                "focused_window": focused,
                "windows": windows,
                "command": command,
            })
            .to_string()
        }
        let mut planner = Planner::new();
        let source = parse_reply(&planner.evaluate(&reconcile_for(
            "stack-seed-1",
            "DP-6",
            serde_json::json!({"x": 0, "y": 0, "w": 1920, "h": 540}),
            "win-1",
            serde_json::json!([
                {"window": "win-1", "output": "DP-6", "workspace": "ws-1",
                 "rect": {"x": 0, "y": 0, "w": 1920, "h": 540}},
            ]),
        )));
        assert_eq!(source["outcome"], "planned", "{source}");
        let target = parse_reply(&planner.evaluate(&reconcile_for(
            "stack-seed-2",
            "HDMI-2",
            serde_json::json!({"x": 0, "y": 540, "w": 1920, "h": 540}),
            "win-2",
            serde_json::json!([
                {"window": "win-2", "output": "HDMI-2", "workspace": "ws-1",
                 "rect": {"x": 0, "y": 540, "w": 1920, "h": 540}},
            ]),
        )));
        assert_eq!(target["outcome"], "planned", "{target}");
        let moved = parse_reply(&planner.evaluate(&stacked_for(
            "stack-move-1",
            "win-1",
            serde_json::json!({"op": "move", "window": "win-1", "direction": "down"}),
        )));
        assert_eq!(moved["outcome"], "planned", "{moved}");
        assert_eq!(moved["detail"]["kind"], "move", "{moved}");
        assert_eq!(moved["detail"]["direction"], "down", "{moved}");
        assert_eq!(moved["detail"]["rule"], "R4", "{moved}");
        assert_eq!(moved["operation"]["target_output"], "HDMI-2", "{moved}");
        assert_eq!(moved["operation"]["source_output"], "DP-6", "{moved}");
        assert_eq!(moved["operation"]["target"], "occupied", "{moved}");
        assert_eq!(moved["desired_focus"]["domain_output"], "HDMI-2", "{moved}");
    }

    #[test]
    fn directional_vertical_pair_requires_reciprocity() {
        // Item 5.2: a one-sided vertical edge refuses as `domain-invalid`
        // (ambiguous topology), and altering vertical adjacency without
        // updating the fingerprint refuses as `fingerprint-mismatch`: the
        // fingerprint binds all four adjacency sides.
        fn stacked_for(
            correlation: &str,
            focused: &str,
            command: serde_json::Value,
            source_adjacent: serde_json::Value,
            target_adjacent: serde_json::Value,
            recompute: bool,
        ) -> String {
            let domains = serde_json::json!([
                {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 0, "w": 1920, "h": 540},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": source_adjacent,
                },
                {
                    "output": "HDMI-2",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 540, "w": 1920, "h": 540},
                    "gap": 8,
                    "outer_gap": 8,
                    "adjacent": target_adjacent,
                },
            ]);
            let windows = serde_json::json!([
                {
                    "window": "win-1",
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "rect": {"x": 0, "y": 0, "w": 1920, "h": 540},
                },
                {
                    "window": "win-2",
                    "output": "HDMI-2",
                    "workspace": "ws-1",
                    "rect": {"x": 0, "y": 540, "w": 1920, "h": 540},
                },
            ]);
            let fingerprint = if recompute {
                let domain_entries: Vec<DirectionalDomainDto> =
                    serde_json::from_value(domains.clone()).expect("domains decode");
                let window_entries: Vec<ObservedDto> =
                    serde_json::from_value(windows.clone()).expect("windows decode");
                directional_fingerprint(&domain_entries, focused, &window_entries)
            } else {
                7
            };
            serde_json::json!({
                "v": 1,
                "correlation_id": correlation,
                "owner": "owner-1",
                "generation": "gen-1",
                "revision": 0,
                "fingerprint": fingerprint,
                "domain": {
                    "output": "DP-6",
                    "workspace": "ws-1",
                    "bounds": {"x": 0, "y": 0, "w": 1920, "h": 540},
                    "gap": 8,
                    "outer_gap": 8,
                },
                "domains": domains,
                "focused_window": focused,
                "windows": windows,
                "command": command,
            })
            .to_string()
        }
        let move_down =
            || serde_json::json!({"op": "move", "window": "win-1", "direction": "down"});
        // One-sided: source names down, target names nothing back.
        let mut planner = Planner::new();
        let one_sided = parse_reply(&planner.evaluate(&stacked_for(
            "stack-recip-1",
            "win-1",
            move_down(),
            serde_json::json!({"down": "HDMI-2"}),
            serde_json::json!({}),
            true,
        )));
        assert_eq!(one_sided["outcome"], "rejected", "{one_sided}");
        assert_eq!(one_sided["detail"], "domain-invalid", "{one_sided}");
        // Wrong-side reciprocity: target answers on left instead of up.
        let mut planner = Planner::new();
        let wrong_side = parse_reply(&planner.evaluate(&stacked_for(
            "stack-recip-2",
            "win-1",
            move_down(),
            serde_json::json!({"down": "HDMI-2"}),
            serde_json::json!({"left": "DP-6"}),
            true,
        )));
        assert_eq!(wrong_side["outcome"], "rejected", "{wrong_side}");
        assert_eq!(wrong_side["detail"], "domain-invalid", "{wrong_side}");
        // Stale fingerprint after adding a vertical edge: refuses before any
        // planning, proving up/down adjacency feeds the fingerprint.
        let mut planner = Planner::new();
        let stale = parse_reply(&planner.evaluate(&stacked_for(
            "stack-recip-3",
            "win-1",
            move_down(),
            serde_json::json!({"down": "HDMI-2"}),
            serde_json::json!({"up": "DP-6"}),
            false,
        )));
        assert_eq!(stale["outcome"], "rejected", "{stale}");
        assert_eq!(stale["detail"], "fingerprint-mismatch", "{stale}");
    }

    #[test]
    fn workspace_send_refuses_cross_output() {
        let mut planner = Planner::new();
        let source = vec![workspace_entry("win-1", "ws-1", 0)];
        let target = vec![workspace_entry("win-t1", "ws-2", 0)];
        let mut request: serde_json::Value = serde_json::from_str(&workspace_request(
            "ws-cross-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            source,
            target,
            workspace_send_body(),
        ))
        .expect("json");
        request["target_domain"]["output"] = serde_json::json!("out-2");
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "cross-output", "{reply}");
    }

    #[test]
    fn workspace_send_refuses_same_workspace() {
        let mut planner = Planner::new();
        let source = vec![workspace_entry("win-1", "ws-1", 0)];
        let target = vec![workspace_entry("win-t1", "ws-2", 0)];
        let mut request: serde_json::Value = serde_json::from_str(&workspace_request(
            "ws-same-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            source,
            target,
            workspace_send_body(),
        ))
        .expect("json");
        request["target_domain"]["workspace"] = serde_json::json!("ws-1");
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "unchanged-workspace", "{reply}");
    }

    #[test]
    fn workspace_send_refuses_absent_focus() {
        let mut planner = Planner::new();
        // No focused window and no source windows: no mover can exist.
        let request = workspace_request(
            "ws-absent-1",
            "owner-1",
            "gen-1",
            0,
            "",
            vec![],
            vec![],
            workspace_send_body(),
        );
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "absent-focus", "{reply}");
    }

    #[test]
    fn workspace_send_refuses_focus_not_observed() {
        let mut planner = Planner::new();
        let source = vec![workspace_entry("win-2", "ws-1", 0)];
        let target = vec![workspace_entry("win-t1", "ws-2", 0)];
        let request = workspace_request(
            "ws-focus-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            source,
            target,
            workspace_send_body(),
        );
        // win-1 is named as focused but only win-2 is observed in the source.
        let reply = parse_reply(&planner.evaluate(&request));
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "snapshot-invalid", "{reply}");
        assert_eq!(reply["detail"], "focused-not-observed", "{reply}");
    }

    #[test]
    fn workspace_send_commits_immediately_with_native_assignment_and_converges() {
        // Compact Planner/Engine codec row: a real send-to-workspace commits
        // immediately with the native membership action (`move-tiled`
        // operation plus preconditions) and both-domain geometry. A failed
        // native assignment then converges on the next complete observations
        // with no retained phantom.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&workspace_request(
            "ws-codec-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_send_body(),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-workspace", "{reply}");
        assert_eq!(reply["operation"]["op"], "move-tiled", "{reply}");
        assert_eq!(reply["operation"]["window"], "win-1", "{reply}");
        assert_eq!(reply["operation"]["source_workspace"], "ws-1", "{reply}");
        assert_eq!(reply["operation"]["target_workspace"], "ws-2", "{reply}");
        assert!(
            reply["preconditions"]
                .as_array()
                .is_some_and(|p| !p.is_empty()),
            "{reply}"
        );
        // Both-domain geometry: the mover lands on the target, the target
        // member survives, and the emptied source carries no entry.
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        let mut members: Vec<(String, String)> = geometry
            .iter()
            .map(|g| {
                (
                    g["window"].as_str().expect("window").to_owned(),
                    g["workspace"].as_str().expect("workspace").to_owned(),
                )
            })
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec![
                ("win-1".to_owned(), "ws-2".to_owned()),
                ("win-t1".to_owned(), "ws-2".to_owned()),
            ],
            "{reply}"
        );
        // Native refused: the next complete observations still show the mover
        // on the source and the target without it; both converge with no
        // retained phantom.
        let source = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "ws-codec-2",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(source["outcome"], "planned", "{source}");
        let target = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "ws-codec-3",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-2",
            "win-t1",
            &[("win-t1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(target["outcome"], "planned", "{target}");
        let source_geometry = source["desired_geometry"].as_array().expect("geometry");
        assert!(
            source_geometry
                .iter()
                .any(|g| g["window"] == "win-1" && g["workspace"] == "ws-1"),
            "{source}"
        );
        let target_geometry = target["desired_geometry"].as_array().expect("geometry");
        assert!(
            target_geometry.iter().all(|g| g["window"] != "win-1"),
            "{target}"
        );
    }

    #[test]
    fn workspace_send_accepts_out_of_bounds_target_observation() {
        // Target observations are host drift too: a valid but out-of-bounds
        // target rect plans and converges, while a malformed one refuses.
        fn target_entry(window: &str, x: i32, y: i32, w: i32, h: i32) -> serde_json::Value {
            serde_json::json!({
                "window": window,
                "output": "out-1",
                "workspace": "ws-2",
                "rect": {"x": x, "y": y, "w": w, "h": h},
            })
        }
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&workspace_request(
            "ws-oob-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![target_entry("win-t1", 1100, 700, 200, 200)],
            workspace_send_body(),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["kind"], "send-to-workspace", "{reply}");
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        let mut members: Vec<(String, String)> = geometry
            .iter()
            .map(|g| {
                (
                    g["window"].as_str().expect("window").to_owned(),
                    g["workspace"].as_str().expect("workspace").to_owned(),
                )
            })
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec![
                ("win-1".to_owned(), "ws-2".to_owned()),
                ("win-t1".to_owned(), "ws-2".to_owned()),
            ],
            "{reply}"
        );
        for entry in geometry {
            let rect = &entry["rect"];
            let (x, y, w, h) = (
                rect["x"].as_i64().unwrap(),
                rect["y"].as_i64().unwrap(),
                rect["w"].as_i64().unwrap(),
                rect["h"].as_i64().unwrap(),
            );
            assert!(x >= 0 && y >= 0, "{reply}");
            assert!(x + w <= 1200 && y + h <= 800, "{reply}");
        }
        let mut planner = Planner::new();
        let malformed = parse_reply(&planner.evaluate(&workspace_request(
            "ws-oob-2",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![target_entry("win-t1", 1100, 700, 0, 200)],
            workspace_send_body(),
        )));
        assert_eq!(malformed["outcome"], "rejected", "{malformed}");
        assert_eq!(malformed["kind"], "snapshot-invalid", "{malformed}");
        assert_eq!(malformed["detail"], "window-rect-invalid", "{malformed}");
    }

    #[test]
    fn plan_summary_ingress_is_bounded_and_redacted() {
        let request = workspace_request(
            "ws-sum-1",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_send_body(),
        );
        let line = summarize_plan_ingress(&request);
        assert_eq!(
            line,
            "omnitiler:plan-summary direction=ingress op=send-to-workspace correlation=ws-sum-1 revision=0"
        );
        // Malformed input degrades to placeholders without echoing anything
        // caller-controlled: no window ids, rects, owner, or payload bytes.
        let garbage = summarize_plan_ingress("{not-json!! owner-1 win-1");
        assert_eq!(
            garbage,
            "omnitiler:plan-summary direction=ingress op=unknown correlation=- revision=-"
        );
        // An invalid correlation shape never echoes, even when well-formed.
        let bad_corr = summarize_plan_ingress(
            r#"{"v":1,"correlation_id":"evil correlation!!","owner":"owner-1","generation":"gen-1","revision":0,"command":{"op":"reconcile"}}"#,
        );
        assert!(bad_corr.contains("correlation=-"), "{bad_corr}");
        assert!(!bad_corr.contains("evil"), "{bad_corr}");
        // Out-of-bounds revisions never echo.
        let bad_rev = summarize_plan_ingress(
            r#"{"v":1,"correlation_id":"ws-sum-1","revision":99999999,"command":{"op":"reconcile"}}"#,
        );
        assert!(bad_rev.contains("revision=-"), "{bad_rev}");
        assert!(!bad_rev.contains("99999999"), "{bad_rev}");
    }

    #[test]
    fn plan_summary_egress_reports_planned_send_truthfully() {
        // Planned send result codes pass through exactly: the native
        // membership action carries the route kind and the committed base.
        let send_request = workspace_request(
            "ws-sum-2",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![workspace_entry("win-1", "ws-1", 0)],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_send_body(),
        );
        let send_reply = r#"{"v":1,"correlation_id":"ws-sum-2","outcome":"planned","kind":"send-to-workspace","base_revision":3}"#;
        assert_eq!(
            summarize_plan_egress(&send_request, send_reply),
            "omnitiler:plan-summary direction=egress op=send-to-workspace correlation=ws-sum-2 outcome=planned kind=send-to-workspace base_revision=3 detail=-"
        );
        // A garbage reply degrades without echoing it.
        let garbage = summarize_plan_egress(&send_request, "{not-json!!");
        assert!(garbage.contains("outcome=unknown"), "{garbage}");
        assert!(garbage.contains("correlation=-"), "{garbage}");
        assert!(!garbage.contains("not-json"), "{garbage}");
        // Snapshot-invalid detail tokens pass through bounded.
        let invalid = summarize_plan_egress(
            &send_request,
            r#"{"v":1,"correlation_id":"ws-sum-2","outcome":"rejected","kind":"snapshot-invalid","detail":"domain-invalid"}"#,
        );
        assert!(invalid.contains("detail=domain-invalid"), "{invalid}");
    }

    #[test]
    fn plan_summary_shape_counts_without_payload_bytes() {
        let request = workspace_request(
            "ws-sum-4",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![
                workspace_entry("win-1", "ws-1", 0),
                workspace_entry("win-2", "ws-1", 100),
            ],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_send_body(),
        );
        let shape = summarize_plan_shape(&request);
        assert!(shape.contains("windows=2"), "{shape}");
        assert!(shape.contains("target_windows=1"), "{shape}");
        assert!(!shape.contains("win-1"), "{shape}");
        assert!(!shape.contains("owner-1"), "{shape}");
        assert!(!shape.contains("\"x\""), "{shape}");
        // Summaries are pure: running them changes no planner state.
        let pre = workspace_request(
            "ws-sum-5",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            vec![
                workspace_entry("win-1", "ws-1", 0),
                workspace_entry("win-2", "ws-1", 100),
            ],
            vec![workspace_entry("win-t1", "ws-2", 0)],
            workspace_send_body(),
        );
        let _ = summarize_plan_ingress(&pre);
        let _ = summarize_plan_shape(&pre);
        let _ = summarize_plan_egress(&pre, &pre);
    }

    fn active_group_request(
        correlation: &str,
        owner: &str,
        generation: &str,
        revision: u64,
        focused: &str,
        windows: &[(&str, i32, i32, i32, i32)],
    ) -> String {
        let entries: Vec<serde_json::Value> = windows
            .iter()
            .map(|(window, x, y, w, h)| {
                serde_json::json!({
                    "window": window,
                    "output": "out-1",
                    "workspace": "ws-1",
                    "rect": {"x": x, "y": y, "w": w, "h": h},
                })
            })
            .collect();
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": owner,
            "generation": generation,
            "revision": revision,
            "fingerprint": 7,
            "domain": {
                "output": "out-1",
                "workspace": "ws-1",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": entries,
            "command": {"op": "active-group"},
        })
        .to_string()
    }

    fn seed_active_group_planner() -> Planner {
        let mut planner = Planner::new();
        for (correlation, focused, windows, command) in [
            (
                "ag-seed-1",
                "win-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "ag-seed-2",
                "win-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let request =
                retained_request(correlation, "owner-1", "gen-1", focused, &windows, command);
            let reply = parse_reply(&planner.evaluate(&request));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        assert_eq!(planner.retained_domains(), 1);
        planner
    }

    #[test]
    fn retained_active_group_returns_parent_members_and_engine_projection() {
        let mut planner = seed_active_group_planner();
        // Retained focus after the second reconcile is win-2.
        let baseline = parse_reply(&planner.evaluate(&retained_request(
            "ag-rec-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(baseline["outcome"], "planned", "{baseline}");
        let expected = geometry_by_window(&baseline);
        // Carried client rectangles drift within bounds; the highlight must
        // still report the retained engine allocation, never native topology.
        let drifted = active_group_request(
            "ag-1",
            "owner-1",
            "gen-1",
            2,
            "win-2",
            &[("win-1", 0, 0, 10, 10), ("win-2", 1100, 700, 50, 50)],
        );
        let reply_text = planner.evaluate(&drifted);
        assert!(reply_text.len() <= PLAN_MAX_REPLY_BYTES, "{reply_text}");
        let reply = parse_reply(&reply_text);
        assert_eq!(reply["outcome"], "active-group", "{reply}");
        assert_eq!(reply["kind"], "active-group", "{reply}");
        assert_eq!(reply["correlation_id"], "ag-1", "{reply}");
        assert_eq!(reply["base_revision"], 2, "{reply}");
        let detail = &reply["detail"];
        assert_eq!(detail["kind"], "active-group", "{reply}");
        assert_eq!(detail["owner"], "owner-1", "{reply}");
        assert_eq!(detail["generation"], "gen-1", "{reply}");
        assert_eq!(detail["domain_output"], "out-1", "{reply}");
        assert_eq!(detail["domain_workspace"], "ws-1", "{reply}");
        assert_eq!(detail["focused_window"], "win-2", "{reply}");
        let group = detail["group"].as_str().expect("group id");
        assert!(!group.is_empty(), "{reply}");
        let members = detail["members"].as_array().expect("members");
        assert_eq!(members.len(), 2, "{reply}");
        let mut windows: Vec<&str> = members
            .iter()
            .map(|m| m["window"].as_str().expect("member window"))
            .collect();
        windows.sort();
        assert_eq!(windows, vec!["win-1", "win-2"], "{reply}");
        for member in members {
            let window = member["window"].as_str().expect("window");
            let rect = (
                member["rect"]["x"].as_i64().unwrap() as i32,
                member["rect"]["y"].as_i64().unwrap() as i32,
                member["rect"]["w"].as_i64().unwrap() as i32,
                member["rect"]["h"].as_i64().unwrap() as i32,
            );
            assert_eq!(rect, expected[window], "{reply} vs {baseline}");
            assert!(
                member["leaf"].as_str().is_some_and(|s| !s.is_empty()),
                "{reply}"
            );
        }
        let bounds = &detail["bounds"];
        let (bx, by, bw, bh) = (
            bounds["x"].as_i64().unwrap() as i32,
            bounds["y"].as_i64().unwrap() as i32,
            bounds["w"].as_i64().unwrap() as i32,
            bounds["h"].as_i64().unwrap() as i32,
        );
        assert!(bw > 0 && bh > 0, "{reply}");
        for member in members {
            let rect = &member["rect"];
            let (x, y, w, h) = (
                rect["x"].as_i64().unwrap() as i32,
                rect["y"].as_i64().unwrap() as i32,
                rect["w"].as_i64().unwrap() as i32,
                rect["h"].as_i64().unwrap() as i32,
            );
            assert!(x >= bx && y >= by, "{reply}");
            assert!(x + w <= bx + bw && y + h <= by + bh, "{reply}");
        }
        let geometry = reply["desired_geometry"].as_array().expect("geometry");
        assert_eq!(geometry.len(), 2, "{reply}");
        assert!(reply["desired_focus"]["leaf"].as_str().is_some(), "{reply}");
        assert_eq!(
            reply["desired_focus"]["leaf"], detail["focused_leaf"],
            "{reply}"
        );
    }

    #[test]
    fn retained_active_group_clears_for_non_group_focus() {
        // Single tiled window is a root leaf: no parent group exists.
        let mut planner = Planner::new();
        let seed = retained_request(
            "ag-solo-seed-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        assert_eq!(parse_reply(&planner.evaluate(&seed))["outcome"], "planned");
        let solo = active_group_request(
            "ag-solo-1",
            "owner-1",
            "gen-1",
            1,
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
        );
        let cleared = parse_reply(&planner.evaluate(&solo));
        assert_eq!(cleared["outcome"], "no-group", "{cleared}");
        assert_eq!(cleared["kind"], "no-group", "{cleared}");
        assert_eq!(cleared["correlation_id"], "ag-solo-1", "{cleared}");
        assert_eq!(cleared["base_revision"], 1, "{cleared}");
        assert_eq!(cleared["detail"]["reason"], "no-parent-group", "{cleared}");
        assert_eq!(cleared["detail"]["generation"], "gen-1", "{cleared}");
        assert!(cleared.get("desired_geometry").is_none(), "{cleared}");
        // Unknown domain has no retained tree: clear without leaking state.
        let unknown = serde_json::json!({
            "v": 1,
            "correlation_id": "ag-unknown-1",
            "owner": "owner-1",
            "generation": "gen-1",
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": "out-9",
                "workspace": "ws-9",
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": "win-1",
            "windows": [{"window": "win-1", "output": "out-9", "workspace": "ws-9",
                "rect": {"x": 0, "y": 0, "w": 100, "h": 80}}],
            "command": {"op": "active-group"},
        })
        .to_string();
        let missing = parse_reply(&planner.evaluate(&unknown));
        assert_eq!(missing["outcome"], "no-group", "{missing}");
        assert_eq!(missing["detail"]["reason"], "no-session", "{missing}");
        assert_eq!(missing["correlation_id"], "ag-unknown-1", "{missing}");
        // Op mismatch inside the handler binds the exact snapshot detail.
        let mut ctx = validate_request(&active_group_request(
            "ag-op-1",
            "owner-1",
            "gen-1",
            1,
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
        ))
        .expect("base valid");
        ctx.request.command["op"] = serde_json::json!("bogus-op");
        let text = planner.evaluate_active_group_retained(&ctx);
        let op_reply = parse_reply(&text);
        assert_eq!(op_reply["outcome"], "rejected", "{op_reply}");
        assert_eq!(op_reply["kind"], "snapshot-invalid", "{op_reply}");
        assert_eq!(op_reply["detail"], "active-group-op-invalid", "{op_reply}");
        assert_eq!(op_reply["correlation_id"], "ag-op-1", "{op_reply}");
        assert!(text.len() <= PLAN_MAX_REPLY_BYTES, "{op_reply}");
    }

    #[test]
    fn retained_active_group_resolves_current_snapshot_without_revision_gate() {
        let mut planner = seed_active_group_planner();
        // Initial/lagging carried revision resolves current retained state
        // instead of livelocking on stale-revision: revision 0 still yields
        // the current base 2 snapshot with safe returned identity.
        let initial = active_group_request(
            "ag-initial-1",
            "owner-1",
            "gen-1",
            0,
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
        );
        let initial_reply = parse_reply(&planner.evaluate(&initial));
        assert_eq!(initial_reply["outcome"], "active-group", "{initial_reply}");
        assert_eq!(
            initial_reply["correlation_id"], "ag-initial-1",
            "{initial_reply}"
        );
        assert_eq!(initial_reply["base_revision"], 2, "{initial_reply}");
        assert_eq!(
            initial_reply["detail"]["owner"], "owner-1",
            "{initial_reply}"
        );
        assert_eq!(
            initial_reply["detail"]["generation"], "gen-1",
            "{initial_reply}"
        );
        assert_eq!(
            initial_reply["detail"]["focused_window"], "win-2",
            "{initial_reply}"
        );
        // Fresh revision with the same observation resolves identically and
        // echoes the exact identity binding.
        let fresh = active_group_request(
            "ag-fresh-1",
            "owner-1",
            "gen-1",
            2,
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
        );
        let fresh_reply = parse_reply(&planner.evaluate(&fresh));
        assert_eq!(fresh_reply["outcome"], "active-group", "{fresh_reply}");
        assert_eq!(fresh_reply["correlation_id"], "ag-fresh-1", "{fresh_reply}");
        assert_eq!(fresh_reply["base_revision"], 2, "{fresh_reply}");
        assert_eq!(
            fresh_reply["detail"]["generation"], "gen-1",
            "{fresh_reply}"
        );
        // Carried-window divergence is safe: drifted rects plus an extra
        // carried-only window still resolve from retained topology with the
        // retained focus binding, never from carried geometry.
        let diverged = active_group_request(
            "ag-diverged-1",
            "owner-1",
            "gen-1",
            0,
            "win-2",
            &[
                ("win-1", 5, 5, 10, 10),
                ("win-2", 1100, 700, 20, 20),
                ("win-9", 0, 0, 50, 50),
            ],
        );
        let diverged_reply = parse_reply(&planner.evaluate(&diverged));
        assert_eq!(
            diverged_reply["outcome"], "active-group",
            "{diverged_reply}"
        );
        assert_eq!(diverged_reply["base_revision"], 2, "{diverged_reply}");
        assert_eq!(
            diverged_reply["detail"]["focused_window"], "win-2",
            "{diverged_reply}"
        );
        // Generation change (adapter restart) discards retained state instead
        // of leaking the previous generation's group.
        let rotated = active_group_request(
            "ag-rot-1",
            "owner-1",
            "gen-2",
            2,
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
        );
        let rotated_reply = parse_reply(&planner.evaluate(&rotated));
        assert_eq!(rotated_reply["outcome"], "no-group", "{rotated_reply}");
        assert_eq!(
            rotated_reply["detail"]["reason"], "no-session",
            "{rotated_reply}"
        );
        assert_eq!(
            rotated_reply["detail"]["generation"], "gen-2",
            "{rotated_reply}"
        );
    }

    #[test]
    fn retained_active_group_aligns_focus_and_resolves_nested_after_move() {
        // Real reconciled/planned lifecycle producing H[win-1, V[win-2, win-3]]:
        // two landscape reconciles build root H, the third reconcile nests V
        // under the tall focused leaf. No hand-seeded sessions.
        let mut planner = Planner::new();
        for (correlation, focused, windows, command) in [
            (
                "ag-nested-1",
                "win-1",
                vec![("win-1", 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "ag-nested-2",
                "win-2",
                vec![("win-1", 0, 0, 100, 80), ("win-2", 200, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            ),
            (
                "ag-nested-3",
                "win-3",
                vec![
                    ("win-1", 0, 0, 100, 80),
                    ("win-2", 200, 0, 100, 80),
                    ("win-3", 400, 0, 100, 80),
                ],
                serde_json::json!({"op": "reconcile"}),
            ),
        ] {
            let request =
                retained_request(correlation, "owner-1", "gen-1", focused, &windows, command);
            let reply = parse_reply(&planner.evaluate(&request));
            assert_eq!(reply["outcome"], "planned", "{reply}");
        }
        // Retained focus is now win-3. An ordinary activation observes win-2
        // (a known tiled window in the same domain). The query must align
        // retained focus from this valid snapshot and resolve the immediate
        // parent inner V[win-2, win-3], not fail-closed `focus-unmapped`.
        let aligned = active_group_request(
            "ag-nested-4",
            "owner-1",
            "gen-1",
            0,
            "win-2",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 80),
            ],
        );
        let aligned_reply = parse_reply(&planner.evaluate(&aligned));
        assert_eq!(aligned_reply["outcome"], "active-group", "{aligned_reply}");
        assert_eq!(aligned_reply["base_revision"], 3, "{aligned_reply}");
        assert_eq!(
            aligned_reply["detail"]["focused_window"], "win-2",
            "{aligned_reply}"
        );
        let mut members: Vec<String> = aligned_reply["detail"]["members"]
            .as_array()
            .expect("members")
            .iter()
            .map(|m| m["window"].as_str().expect("window").to_owned())
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec!["win-2".to_owned(), "win-3".to_owned()],
            "{aligned_reply}"
        );
        // Root H proof: focusing win-1 resolves the 3-member root group.
        let root = active_group_request(
            "ag-nested-5",
            "owner-1",
            "gen-1",
            0,
            "win-1",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 80),
            ],
        );
        let root_reply = parse_reply(&planner.evaluate(&root));
        assert_eq!(root_reply["outcome"], "active-group", "{root_reply}");
        assert_eq!(
            root_reply["detail"]["members"].as_array().map(Vec::len),
            Some(3),
            "{root_reply}"
        );
        // Move the inner member down (swap within V). Focus must remain win-2
        // and the post-move query must still yield the inner active group.
        let moved = retained_request(
            "ag-nested-6",
            "owner-1",
            "gen-1",
            "win-2",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 80),
            ],
            serde_json::json!({"op": "move", "window": "win-2", "direction": "down"}),
        );
        let moved_reply = parse_reply(&planner.evaluate(&moved));
        assert_eq!(moved_reply["outcome"], "planned", "{moved_reply}");
        let focus_leaf = moved_reply["desired_focus"]["leaf"]
            .as_str()
            .expect("focus leaf");
        let mut focus_window = String::new();
        for entry in moved_reply["desired_geometry"]
            .as_array()
            .expect("geometry")
        {
            if entry["leaf"].as_str() == Some(focus_leaf) {
                focus_window = entry["window"].as_str().expect("window").to_owned();
            }
        }
        assert_eq!(focus_window, "win-2", "{moved_reply}");
        let after = active_group_request(
            "ag-nested-7",
            "owner-1",
            "gen-1",
            0,
            "win-2",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 200, 0, 100, 80),
                ("win-3", 400, 0, 100, 80),
            ],
        );
        let after_reply = parse_reply(&planner.evaluate(&after));
        assert_eq!(after_reply["outcome"], "active-group", "{after_reply}");
        assert_eq!(
            after_reply["detail"]["focused_window"], "win-2",
            "{after_reply}"
        );
        let mut after_members: Vec<String> = after_reply["detail"]["members"]
            .as_array()
            .expect("members")
            .iter()
            .map(|m| m["window"].as_str().expect("window").to_owned())
            .collect();
        after_members.sort();
        assert_eq!(
            after_members,
            vec!["win-2".to_owned(), "win-3".to_owned()],
            "{after_reply}"
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn retained_request_for_domain(
        correlation: &str,
        owner: &str,
        generation: &str,
        output: &str,
        workspace: &str,
        focused: &str,
        windows: &[(&str, i32, i32, i32, i32)],
        command: serde_json::Value,
    ) -> String {
        let entries: Vec<serde_json::Value> = windows
            .iter()
            .map(|(window, x, y, w, h)| {
                serde_json::json!({
                    "window": window,
                    "output": output,
                    "workspace": workspace,
                    "rect": {"x": x, "y": y, "w": w, "h": h},
                })
            })
            .collect();
        serde_json::json!({
            "v": 1,
            "correlation_id": correlation,
            "owner": owner,
            "generation": generation,
            "revision": 0,
            "fingerprint": 7,
            "domain": {
                "output": output,
                "workspace": workspace,
                "bounds": {"x": 0, "y": 0, "w": 1200, "h": 800},
                "gap": 0,
                "outer_gap": 0,
            },
            "focused_window": focused,
            "windows": entries,
            "command": command,
        })
        .to_string()
    }

    #[test]
    fn retained_empty_reconcile_retires_session_and_frees_slot() {
        // D23 tentative: an empty complete observation retains the session
        // with stored minimize slots (omission is ambiguous at the converge
        // layer); explicit release-domain retires the slot so a later
        // background domain can be admitted.
        // Offline only: retained Planner evaluation, no bus.
        let mut planner = Planner::new();
        let seed = retained_request_for_domain(
            "empty-retire-1",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-2",
            "win-h",
            &[("win-h", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let seed_reply = parse_reply(&planner.evaluate(&seed));
        assert_eq!(seed_reply["outcome"], "planned", "{seed_reply}");
        assert_eq!(planner.retained_domains(), 1);
        let empty = retained_request_for_domain(
            "empty-retire-2",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-2",
            "",
            &[],
            serde_json::json!({"op": "reconcile"}),
        );
        let empty_reply = parse_reply(&planner.evaluate(&empty));
        assert_eq!(empty_reply["outcome"], "planned", "{empty_reply}");
        assert_eq!(
            empty_reply["desired_geometry"].as_array().map(Vec::len),
            Some(0),
            "{empty_reply}"
        );
        assert!(empty_reply["desired_focus"].is_null(), "{empty_reply}");
        assert_eq!(planner.retained_domains(), 1);
        // Explicit removal retires the retained slot.
        let release = retained_request_for_domain(
            "empty-retire-release",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-2",
            "",
            &[],
            serde_json::json!({"op": "release-domain"}),
        );
        let release_reply = parse_reply(&planner.evaluate(&release));
        assert_eq!(release_reply["outcome"], "released", "{release_reply}");
        assert_eq!(release_reply["kind"], "release-domain", "{release_reply}");
        assert_eq!(planner.retained_domains(), 0);
        // The freed slot admits a subsequent background domain.
        let next = retained_request_for_domain(
            "empty-retire-3",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-3",
            "win-n",
            &[("win-n", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let next_reply = parse_reply(&planner.evaluate(&next));
        assert_eq!(next_reply["outcome"], "planned", "{next_reply}");
        assert_eq!(planner.retained_domains(), 1);
    }

    #[test]
    fn retained_many_domains_plan_without_count_cap() {
        // No retained-domain count cap: well beyond the old 16-domain bound,
        // every distinct domain converges and stays retained. Offline only.
        let mut planner = Planner::new();
        for index in 1..=24 {
            let workspace = format!("ws-{index}");
            let window = format!("win-{index}");
            let correlation = format!("many-domain-reconcile-{index}");
            let request = retained_request_for_domain(
                &correlation,
                "owner-1",
                "gen-1",
                "out-1",
                &workspace,
                &window,
                &[(window.as_str(), 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            );
            let reply = parse_reply(&planner.evaluate(&request));
            assert_eq!(reply["outcome"], "planned", "{reply} {index}");
        }
        assert_eq!(planner.retained_domains(), 24);
        // D23 tentative: an empty complete observation retains the session
        // with stored minimize slots instead of retiring it.
        let empty = retained_request_for_domain(
            "many-domain-empty-1",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-1",
            "",
            &[],
            serde_json::json!({"op": "reconcile"}),
        );
        let empty_reply = parse_reply(&planner.evaluate(&empty));
        assert_eq!(empty_reply["outcome"], "planned", "{empty_reply}");
        assert_eq!(
            empty_reply["desired_geometry"].as_array().map(Vec::len),
            Some(0),
            "{empty_reply}"
        );
        assert!(empty_reply["desired_focus"].is_null(), "{empty_reply}");
        assert_eq!(planner.retained_domains(), 24);
        // Genuine retire still occurs via explicit removal.
        let retire = retained_request_for_domain(
            "many-domain-retire-1",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-1",
            "",
            &[],
            serde_json::json!({"op": "release-domain"}),
        );
        let retire_reply = parse_reply(&planner.evaluate(&retire));
        assert_eq!(retire_reply["outcome"], "released", "{retire_reply}");
        assert_eq!(retire_reply["kind"], "release-domain", "{retire_reply}");
        assert_eq!(planner.retained_domains(), 23);
        // No count cap: a further background domain is still admitted.
        let next = retained_request_for_domain(
            "many-domain-admit-1",
            "owner-1",
            "gen-1",
            "out-1",
            "ws-25",
            "win-25",
            &[("win-25", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        let next_reply = parse_reply(&planner.evaluate(&next));
        assert_eq!(next_reply["outcome"], "planned", "{next_reply}");
        assert_eq!(planner.retained_domains(), 24);
    }

    #[test]
    fn request_size_cap_is_one_mib_with_bounded_oversize_refusal() {
        assert_eq!(PLAN_MAX_REQUEST_BYTES, 1_048_576);
        assert_eq!(PLAN_MAX_REPLY_BYTES, 64 * 1024);
        // At-cap input passes the size gate (trailing JSON whitespace is
        // ignored by parsing, so padding is neutral).
        let base = retained_request(
            "size-cap-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        );
        assert!(base.len() < PLAN_MAX_REQUEST_BYTES);
        let mut at_cap = base.clone();
        at_cap.push_str(&" ".repeat(PLAN_MAX_REQUEST_BYTES - at_cap.len()));
        assert_eq!(at_cap.len(), PLAN_MAX_REQUEST_BYTES);
        let mut planner = Planner::new();
        let at_reply = parse_reply(&planner.evaluate(&at_cap));
        assert_ne!(at_reply["kind"], "oversized", "{at_reply}");
        // Just-over-cap input refuses without parsing: empty correlation,
        // bounded fixed message, valid JSON within the reply cap.
        let mut over = base.clone();
        over.push_str(&" ".repeat(PLAN_MAX_REQUEST_BYTES - over.len() + 1));
        assert_eq!(over.len(), PLAN_MAX_REQUEST_BYTES + 1);
        let over_text = planner.evaluate(&over);
        let over_reply: serde_json::Value =
            serde_json::from_str(&over_text).expect("oversize reply is JSON");
        assert_eq!(over_reply["outcome"], "rejected");
        assert_eq!(over_reply["kind"], "oversized");
        assert_eq!(over_reply["message"], "request exceeds size bound");
        assert_eq!(over_reply["correlation_id"], "");
        assert!(over_text.len() <= PLAN_MAX_REPLY_BYTES);
        // The oversize path never echoes untrusted bytes, so no trusted
        // correlation can be extracted: the service early-exit stays fixed
        // and uncorrelated (adapter logs the correlated preflight refusal).
        assert!(!over_text.contains("size-cap"));
    }

    #[test]
    fn oversize_reply_from_valid_large_request_stays_correlated_and_bounded() {
        // A valid large request (800 observed windows, well under the 1 MiB
        // request cap) overflows the 64 KiB reply cap through desired
        // geometry. The codec fallback must stay correlated (validated
        // correlation only, never untrusted bytes) and bounded, and the
        // normal summary pair must stay bounded with the correlation
        // attributable.
        let names: Vec<String> = (0..800).map(|i| format!("win-{i}")).collect();
        let windows: Vec<(&str, i32, i32, i32, i32)> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.as_str(), i as i32, 0, 1, 800))
            .collect();
        let focused = names.last().expect("names").clone();
        let request = retained_request(
            "big-reply-1",
            "owner-1",
            "gen-1",
            &focused,
            &windows,
            serde_json::json!({"op": "reconcile"}),
        );
        assert!(request.len() < PLAN_MAX_REQUEST_BYTES, "{}", request.len());
        let mut planner = Planner::new();
        let text = planner.evaluate(&request);
        let reply: serde_json::Value = serde_json::from_str(&text).expect("fallback reply is JSON");
        assert_eq!(reply["outcome"], "rejected", "{reply}");
        assert_eq!(reply["kind"], "reply-oversize", "{reply}");
        assert_eq!(reply["message"], "reply exceeds size bound", "{reply}");
        assert_eq!(reply["correlation_id"], "big-reply-1", "{reply}");
        assert!(text.len() <= PLAN_MAX_REPLY_BYTES, "{reply}");
        // No carried data echoes: only the validated correlation survives.
        assert!(!text.contains("win-42"), "{reply}");
        let ingress = summarize_plan_ingress(&request);
        let egress = summarize_plan_egress(&request, &text);
        for line in [&ingress, &egress] {
            assert!(line.len() <= 512, "{line}");
        }
        assert!(ingress.contains("correlation=big-reply-1"), "{ingress}");
        assert!(egress.contains("correlation=big-reply-1"), "{egress}");
        assert!(egress.contains("kind=reply-oversize"), "{egress}");
    }

    fn fit_excluded_request(
        correlation: &str,
        focused: &str,
        windows: &[(&str, i32, i32, i32, i32)],
        excluded: &[&str],
        command: serde_json::Value,
    ) -> String {
        let mut request: serde_json::Value = serde_json::from_str(&retained_request(
            correlation,
            "owner-1",
            "gen-1",
            focused,
            windows,
            command,
        ))
        .expect("valid retained request");
        for entry in request["windows"].as_array_mut().expect("windows") {
            if excluded.contains(&entry["window"].as_str().expect("window")) {
                entry["fit_excluded"] = serde_json::Value::Bool(true);
            }
        }
        request.to_string()
    }

    #[test]
    fn adoption_fit_horizontal_nary_commits_through_reconcile() {
        // Unequal 400/800 side-by-side N-ary cut: the normal seed would
        // reflow to an equal split, so exact observed geometry proves the
        // recursive-cut fit path.
        let windows = [("win-1", 0, 0, 400, 800), ("win-2", 400, 0, 800, 800)];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "fit-h-1",
            "owner-1",
            "gen-1",
            "win-2",
            &windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 400, 800)),
                ("win-2".to_owned(), (400, 0, 800, 800)),
            ]),
            "{reply}"
        );
        assert_eq!(
            planner.engine.last_adoption_fit().unwrap().outcome,
            "fitted"
        );
    }

    #[test]
    fn adoption_fit_vertical_nary_with_exact_shares() {
        // Three-high stacked N-ary cut: one N-ary group with positive spans
        // as shares projects back to the exact observed geometry.
        let windows = [
            ("win-1", 0, 0, 1200, 200),
            ("win-2", 0, 200, 1200, 200),
            ("win-3", 0, 400, 1200, 400),
        ];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "fit-v-1",
            "owner-1",
            "gen-1",
            "win-3",
            &windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["base_revision"], 0, "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2", "win-3"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 1200, 200)),
                ("win-2".to_owned(), (0, 200, 1200, 200)),
                ("win-3".to_owned(), (0, 400, 1200, 400)),
            ]),
            "{reply}"
        );
    }

    #[test]
    fn adoption_fit_respects_configured_gaps() {
        let windows = [("win-1", 8, 8, 588, 784), ("win-2", 604, 8, 588, 784)];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request_with_selected_gaps(
            "fit-gap-1",
            "owner-1",
            "gen-1",
            "win-2",
            &windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["base_revision"], 0, "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        let got = geometry_by_window(&reply);
        assert_eq!(got["win-1"], (8, 8, 588, 784), "{reply}");
        assert_eq!(got["win-2"], (604, 8, 588, 784), "{reply}");
        assert_eq!(
            got["win-2"].0,
            got["win-1"].0 + got["win-1"].2 + 8,
            "{reply}"
        );
    }

    #[test]
    fn adoption_fit_horizontal_nary_with_drift_projects_canonical() {
        // Imperfect horizontal N-ary cut: left/right edge offsets, cross-axis
        // drift, and a nonconfigured observed 7px inter-gap. The x intervals
        // stay sequential within tolerance, so the fit builds one N-ary group
        // with the observed widths as shares and projects the canonical
        // configured-gap result, which matches neither the observed geometry
        // nor the normal equal reflow.
        let windows = [("win-1", 10, 5, 398, 790), ("win-2", 415, 2, 770, 795)];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "fit-near-h-1",
            "owner-1",
            "gen-1",
            "win-2",
            &windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["base_revision"], 0, "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        let got = geometry_by_window(&reply);
        assert_eq!(
            got,
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 409, 800)),
                ("win-2".to_owned(), (409, 0, 791, 800)),
            ]),
            "{reply}"
        );
        assert_ne!(
            got,
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (10, 5, 398, 790)),
                ("win-2".to_owned(), (415, 2, 770, 795)),
            ]),
            "{reply}"
        );
        assert_ne!(
            got,
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 600, 800)),
                ("win-2".to_owned(), (600, 0, 600, 800)),
            ]),
            "{reply}"
        );
    }

    #[test]
    fn adoption_fit_vertical_nary_with_drift_projects_canonical() {
        // Imperfect vertical N-ary cut: cross-axis drift with x intervals
        // overlapping (so horizontal is unsupported) while y intervals stay
        // sequential within tolerance. Canonical heights come from the
        // observed spans.
        let windows = [("win-1", 5, 10, 1190, 250), ("win-2", 2, 270, 1194, 515)];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "fit-near-v-1",
            "owner-1",
            "gen-1",
            "win-2",
            &windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_eq!(reply["base_revision"], 0, "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 1200, 261)),
                ("win-2".to_owned(), (0, 261, 1200, 539)),
            ]),
            "{reply}"
        );
    }

    #[test]
    fn adoption_fit_excluded_declines_to_seed() {
        // The marker only declines fitting: the normal seed still tiles the
        // flagged member to the same deterministic geometry.
        let windows = [("win-1", 0, 0, 400, 800), ("win-2", 400, 0, 800, 800)];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&fit_excluded_request(
            "fit-x-1",
            "win-2",
            &windows,
            &["win-1"],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 600, 800)),
                ("win-2".to_owned(), (600, 0, 600, 800)),
            ]),
            "{reply}"
        );
    }

    #[test]
    fn adoption_fit_commits_once_and_retained_followup_never_refits() {
        let mut planner = Planner::new();
        let fitted = parse_reply(&planner.evaluate(&retained_request(
            "fit-r-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 400, 800), ("win-2", 400, 0, 800, 800)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(fitted["outcome"], "planned", "{fitted}");
        let before = geometry_by_window(&fitted)["win-1"];
        // Fresh attempt records exactly one fitted report.
        let fresh_report = planner.engine.last_adoption_fit().expect("fresh fit logs");
        assert_eq!(fresh_report.outcome, "fitted", "{fitted}");
        assert_eq!(fresh_report.reason, "ok", "{fitted}");
        assert_eq!(fresh_report.windows, 2, "{fitted}");
        assert_eq!(fresh_report.correlation.as_str(), "fit-r-1", "{fitted}");
        // A retained follow-up reconciles the newcomer into the fitted tree
        // without rewriting the fitted first child.
        let follow = parse_reply(&planner.evaluate(&retained_request(
            "fit-r-2",
            "owner-1",
            "gen-1",
            "win-3",
            &[
                ("win-1", 0, 0, 400, 800),
                ("win-2", 400, 0, 800, 800),
                ("win-3", 0, 0, 100, 80),
            ],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(follow["outcome"], "planned", "{follow}");
        assert_geometry_covers(&follow, &["win-1", "win-2", "win-3"]);
        assert_eq!(
            geometry_by_window(&follow)["win-1"],
            before,
            "{follow} vs {fitted}"
        );
        // Retained reconciliations never duplicate the adoption-fit log.
        assert!(planner.engine.last_adoption_fit().is_none(), "{follow}");
    }

    #[test]
    fn adoption_fit_nested_left_two_right_is_focus_independent_and_exact() {
        // Nested recursive cut: left plus a two-high right column. Observed
        // spans become shares, so the canonical projection matches the
        // observation with zero moves regardless of which member is focused.
        let windows = [
            ("win-left", 0, 0, 400, 800),
            ("win-top", 400, 0, 800, 400),
            ("win-bottom", 400, 400, 800, 400),
        ];
        let mut baseline: Option<std::collections::BTreeMap<String, (i32, i32, i32, i32)>> = None;
        for focused in ["win-left", "win-top", "win-bottom"] {
            let mut planner = Planner::new();
            let reply = parse_reply(&planner.evaluate(&retained_request(
                "adopt-nested-1",
                "owner-1",
                "gen-1",
                focused,
                &windows,
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(reply["outcome"], "planned", "{reply}");
            assert_geometry_covers(&reply, &["win-left", "win-top", "win-bottom"]);
            let got = geometry_by_window(&reply);
            assert_eq!(
                got,
                std::collections::BTreeMap::from([
                    ("win-left".to_owned(), (0, 0, 400, 800)),
                    ("win-top".to_owned(), (400, 0, 800, 400)),
                    ("win-bottom".to_owned(), (400, 400, 800, 400)),
                ]),
                "{reply} focused={focused}"
            );
            if let Some(first) = &baseline {
                assert_eq!(&got, first, "focused={focused}");
            } else {
                baseline = Some(got);
            }
            let report = planner.engine.last_adoption_fit().expect("fresh fit logs");
            assert_eq!(report.outcome, "fitted", "{reply}");
            assert_eq!(report.reason, "ok", "{reply}");
            assert_eq!(report.windows, 3, "{reply}");
            assert_eq!(report.centre_splits, 0, "{reply}");
        }
    }

    #[test]
    fn adoption_fit_nested_resized_shares_follow_observed_spans() {
        // Same nested topology with resized observed spans: shares follow the
        // observed primary spans, not an equal split.
        let windows = [
            ("win-left", 0, 0, 600, 800),
            ("win-top", 600, 0, 600, 500),
            ("win-bottom", 600, 500, 600, 300),
        ];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "adopt-nested-resize-1",
            "owner-1",
            "gen-1",
            "win-top",
            &windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        // Shares follow the observed 500/300 spans through the canonical
        // proportional projection (one-unit reservation), not an equal
        // 400/400 split.
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-left".to_owned(), (0, 0, 600, 800)),
                ("win-top".to_owned(), (600, 0, 600, 499)),
                ("win-bottom".to_owned(), (600, 499, 600, 301)),
            ]),
            "{reply}"
        );
    }

    #[test]
    fn adoption_fit_nested_gap_projection_is_canonical() {
        // Nested cut under configured gaps: the canonical projection keeps
        // the outer inset and inner gaps with zero retained moves.
        let mut planner = Planner::new();
        let request: serde_json::Value =
            serde_json::from_str(&retained_request_with_selected_gaps(
                "adopt-nested-gap-1",
                "owner-1",
                "gen-1",
                "win-top",
                &[
                    ("win-left", 8, 8, 388, 784),
                    ("win-top", 404, 8, 788, 388),
                    ("win-bottom", 404, 404, 788, 388),
                ],
                serde_json::json!({"op": "reconcile"}),
            ))
            .expect("valid request");
        let reply = parse_reply(&planner.evaluate(&request.to_string()));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-left", "win-top", "win-bottom"]);
        let got = geometry_by_window(&reply);
        assert_eq!(
            got,
            std::collections::BTreeMap::from([
                ("win-left".to_owned(), (8, 8, 388, 784)),
                ("win-top".to_owned(), (404, 8, 788, 388)),
                ("win-bottom".to_owned(), (404, 404, 788, 388)),
            ]),
            "{reply}"
        );
        let left = got["win-left"];
        let top = got["win-top"];
        let bottom = got["win-bottom"];
        assert_eq!(left.0, 8, "{reply}");
        assert_eq!(left.1, 8, "{reply}");
        assert_eq!(top.0, left.0 + left.2 + 8, "{reply}");
        assert_eq!(bottom.0, top.0, "{reply}");
        assert_eq!(bottom.1, top.1 + top.3 + 8, "{reply}");
        // Retained follow-up with the same observation is a zero-move
        // projection on the fitted tree.
        let follow = parse_reply(
            &planner.evaluate(
                &request
                    .to_string()
                    .replace("adopt-nested-gap-1", "adopt-nested-gap-2"),
            ),
        );
        assert_eq!(follow["outcome"], "planned", "{follow}");
        assert_eq!(geometry_by_window(&follow), got, "{follow}");
        assert!(planner.engine.last_adoption_fit().is_none(), "{follow}");
    }

    #[test]
    fn adoption_fit_overlap_within_tolerance_fits_and_beyond_declines() {
        // Domain 1200 wide: tolerance is max(0, 36) = 36. A 5px cross-cut
        // overlap still fits clean; a 200px overlap has no valid cut and
        // centre-splits on the largest sorted centre gap.
        let mut fitted_planner = Planner::new();
        let fitted = parse_reply(&fitted_planner.evaluate(&retained_request(
            "adopt-tol-1",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 400, 800), ("win-2", 395, 0, 800, 800)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(fitted["outcome"], "planned", "{fitted}");
        assert_eq!(
            geometry_by_window(&fitted),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 400, 800)),
                ("win-2".to_owned(), (400, 0, 800, 800)),
            ]),
            "{fitted}"
        );
        let report = fitted_planner.engine.last_adoption_fit().expect("fit logs");
        assert_eq!(report.outcome, "fitted", "{fitted}");
        assert_eq!(report.reason, "ok", "{fitted}");
        assert_eq!(report.centre_splits, 0, "{fitted}");

        let mut split_planner = Planner::new();
        let fallback = parse_reply(&split_planner.evaluate(&retained_request(
            "adopt-tol-2",
            "owner-1",
            "gen-1",
            "win-2",
            &[("win-1", 0, 0, 600, 800), ("win-2", 400, 0, 600, 800)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(fallback["outcome"], "planned", "{fallback}");
        assert_eq!(
            geometry_by_window(&fallback),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 600, 800)),
                ("win-2".to_owned(), (600, 0, 600, 800)),
            ]),
            "{fallback}"
        );
        let fallback_report = split_planner
            .engine
            .last_adoption_fit()
            .expect("split logs");
        // Overlap beyond tolerance declines the centre-split fit to the
        // normal sequential seed (equal long-edge split here).
        assert_eq!(fallback_report.outcome, "fallback", "{fallback}");
        assert_eq!(fallback_report.reason, "centre_split", "{fallback}");
        assert_eq!(fallback_report.centre_splits, 1, "{fallback}");
    }

    #[test]
    fn adoption_fit_cascade_declines_to_sequential_seed() {
        // Overlapping cascades need a centre split (never-tiled input), so
        // they decline to the normal deterministic sequential long-edge
        // seed. Per focus the result is input-order independent and covers
        // the domain; across focuses the focus-last order may differ.
        let horizontal = [
            ("win-1", 0, 0, 600, 600),
            ("win-2", 200, 50, 600, 600),
            ("win-3", 400, 100, 600, 600),
            ("win-4", 600, 150, 600, 600),
        ];
        let vertical = [
            ("win-1", 0, 0, 1000, 400),
            ("win-2", 50, 150, 1000, 400),
            ("win-3", 100, 300, 1000, 400),
        ];
        for (tag, windows, _horizontal_axis) in [
            ("adopt-cascade-h", horizontal.as_slice(), true),
            ("adopt-cascade-v", vertical.as_slice(), false),
        ] {
            for focused in windows.iter().map(|w| w.0) {
                let mut per_focus_baseline = None;
                for reversed in [false, true] {
                    let mut ordered = windows.to_vec();
                    if reversed {
                        ordered.reverse();
                    }
                    let mut planner = Planner::new();
                    let reply = parse_reply(&planner.evaluate(&retained_request(
                        tag,
                        "owner-1",
                        "gen-1",
                        focused,
                        &ordered,
                        serde_json::json!({"op": "reconcile"}),
                    )));
                    assert_eq!(reply["outcome"], "planned", "{reply}");
                    let got = geometry_by_window(&reply);
                    let names: Vec<&str> = windows.iter().map(|w| w.0).collect();
                    assert_geometry_covers(&reply, &names);
                    if let Some(first) = &per_focus_baseline {
                        assert_eq!(&got, first, "focused={focused} reversed={reversed}");
                    } else {
                        per_focus_baseline = Some(got);
                    }
                    let report = planner.engine.last_adoption_fit().expect("fit logs");
                    assert_eq!(report.outcome, "fallback", "{reply}");
                    assert_eq!(report.reason, "centre_split", "{reply}");
                    assert!(report.centre_splits > 0, "{reply}");
                }
            }
        }
    }

    #[test]
    fn adoption_fit_real5_cascade_declines_to_sequential_seed() {
        // Real5 startup (run-01dd532e5c7fa523) with native minimums (Calc
        // 402x627, Paint 864x617, NPs 401x246) on domain (8,8,2544,1364) gap
        // 8. Overlap declines to the sequential seed; each focus below carries
        // its exact allocation with the exact remaining overconstrained set
        // (only a focused Paint still strands Calc+Paint).
        let windows = [
            ("win-calc", 383, 375, 734, 805),
            ("win-paint", 83, 75, 1234, 1042),
            ("win-np1", 157, 150, 961, 718),
            ("win-np2", 232, 225, 961, 718),
            ("win-np3", 307, 300, 961, 718),
        ];
        let mins: std::collections::BTreeMap<&str, (i32, i32)> = [
            ("win-calc", (402, 627)),
            ("win-paint", (864, 617)),
            ("win-np1", (401, 246)),
            ("win-np2", (401, 246)),
            ("win-np3", (401, 246)),
        ]
        .into_iter()
        .collect();
        let expected: &[(&str, &[TileExpect<'_>])] = &[
            (
                "win-calc",
                &[
                    ("win-paint", 8, 8, 1268, 1364, false),
                    ("win-np1", 1284, 8, 1268, 475, false),
                    ("win-np2", 1284, 491, 630, 881, false),
                    ("win-np3", 1922, 491, 630, 246, false),
                    ("win-calc", 1922, 745, 630, 627, false),
                ],
            ),
            (
                "win-paint",
                &[
                    ("win-np1", 8, 8, 1263, 1364, false),
                    ("win-np2", 1279, 8, 1273, 678, false),
                    ("win-np3", 1279, 694, 401, 678, false),
                    ("win-calc", 1688, 694, 864, 335, true),
                    ("win-paint", 1688, 1037, 864, 335, true),
                ],
            ),
            (
                "win-np1",
                &[
                    ("win-paint", 8, 8, 1268, 1364, false),
                    ("win-np2", 1284, 8, 1268, 475, false),
                    ("win-np3", 1284, 491, 630, 881, false),
                    ("win-calc", 1922, 491, 630, 627, false),
                    ("win-np1", 1922, 1126, 630, 246, false),
                ],
            ),
            (
                "win-np2",
                &[
                    ("win-paint", 8, 8, 1268, 1364, false),
                    ("win-np1", 1284, 8, 1268, 475, false),
                    ("win-np3", 1284, 491, 630, 881, false),
                    ("win-calc", 1922, 491, 630, 627, false),
                    ("win-np2", 1922, 1126, 630, 246, false),
                ],
            ),
            (
                "win-np3",
                &[
                    ("win-paint", 8, 8, 1268, 1364, false),
                    ("win-np1", 1284, 8, 1268, 475, false),
                    ("win-np2", 1284, 491, 630, 881, false),
                    ("win-calc", 1922, 491, 630, 627, false),
                    ("win-np3", 1922, 1126, 630, 246, false),
                ],
            ),
        ];
        for (focused, tiles) in expected {
            for reversed in [false, true] {
                let mut ordered = windows.to_vec();
                if reversed {
                    ordered.reverse();
                }
                let base = custom_request(
                    "adopt-real5-1",
                    focused,
                    (8, 8, 2544, 1364),
                    &ordered,
                    serde_json::json!({"op": "reconcile"}),
                );
                let mut value: serde_json::Value =
                    serde_json::from_str(&base).expect("valid request");
                value["domain"]["gap"] = serde_json::json!(8);
                for entry in value["windows"].as_array_mut().expect("windows") {
                    let id = entry["window"].as_str().expect("id").to_owned();
                    let (mw, mh) = mins[id.as_str()];
                    entry["min_size"] = serde_json::json!({"w": mw, "h": mh});
                }
                let mut planner = Planner::new();
                let reply = parse_reply(&planner.evaluate(&value.to_string()));
                assert_eq!(reply["outcome"], "planned", "{reply}");
                let names: Vec<&str> = windows.iter().map(|w| w.0).collect();
                assert_geometry_covers(&reply, &names);
                let got: std::collections::BTreeMap<String, (i32, i32, i32, i32, bool)> =
                    reply["desired_geometry"]
                        .as_array()
                        .expect("geometry")
                        .iter()
                        .map(|entry| {
                            let rect = &entry["rect"];
                            (
                                entry["window"].as_str().expect("window").to_owned(),
                                (
                                    rect["x"].as_i64().unwrap() as i32,
                                    rect["y"].as_i64().unwrap() as i32,
                                    rect["w"].as_i64().unwrap() as i32,
                                    rect["h"].as_i64().unwrap() as i32,
                                    entry.get("overconstrained")
                                        == Some(&serde_json::Value::Bool(true)),
                                ),
                            )
                        })
                        .collect();
                let want: std::collections::BTreeMap<String, (i32, i32, i32, i32, bool)> = tiles
                    .iter()
                    .map(|(w, x, y, ww, h, f)| ((*w).to_owned(), (*x, *y, *ww, *h, *f)))
                    .collect();
                assert_eq!(got, want, "{reply} focused={focused} reversed={reversed}");
                let report = planner.engine.last_adoption_fit().expect("fit logs");
                assert_eq!(report.outcome, "fallback", "{reply}");
                assert_eq!(report.reason, "centre_split", "{reply}");
                assert_eq!(report.centre_splits, 4, "{reply}");
            }
        }
    }

    #[test]
    fn adoption_fit_4cascade_with_real_mins_yields_bisection_chain() {
        // Four overlapping windows decline to the sequential seed, which
        // yields a bisection chain rather than a symmetric 2x2: the first
        // window keeps a full-height half while the remainder subdivides.
        // Minimums reuse the real5 native classes (NP 401x246, Calc 402x627,
        // Paint 864x617); narrow 300-wide leaves flag exactly.
        let windows = [
            ("win-1", 0, 0, 600, 600),
            ("win-2", 200, 50, 600, 600),
            ("win-3", 400, 100, 600, 600),
            ("win-4", 600, 150, 600, 600),
        ];
        let hints: std::collections::BTreeMap<&str, HintPair> = [
            ("win-1", (Some((401, 246)), None)),
            ("win-2", (Some((402, 627)), None)),
            ("win-3", (Some((401, 246)), None)),
            ("win-4", (Some((864, 617)), None)),
        ]
        .into_iter()
        .collect();
        let expected: &[(&str, &[TileExpect<'_>])] = &[
            (
                "win-1",
                &[
                    ("win-2", 0, 0, 600, 800, false),
                    ("win-3", 600, 0, 600, 400, false),
                    ("win-4", 600, 400, 300, 400, true),
                    ("win-1", 900, 400, 300, 400, true),
                ],
            ),
            (
                "win-4",
                &[
                    ("win-1", 0, 0, 600, 800, false),
                    ("win-2", 600, 0, 600, 400, true),
                    ("win-3", 600, 400, 300, 400, true),
                    ("win-4", 900, 400, 300, 400, true),
                ],
            ),
        ];
        for (focused, tiles) in expected {
            let mut per_focus = None;
            for reversed in [false, true] {
                let mut ordered = windows.to_vec();
                if reversed {
                    ordered.reverse();
                }
                let mut planner = Planner::new();
                let reply = parse_reply(&planner.evaluate(&retained_request_with_hints(
                    "adopt-4casc-1",
                    "owner-1",
                    "gen-1",
                    focused,
                    &ordered,
                    &hints,
                    serde_json::json!({"op": "reconcile"}),
                )));
                assert_eq!(reply["outcome"], "planned", "{reply}");
                let names: Vec<&str> = windows.iter().map(|w| w.0).collect();
                assert_geometry_covers(&reply, &names);
                let got: std::collections::BTreeMap<String, (i32, i32, i32, i32, bool)> =
                    reply["desired_geometry"]
                        .as_array()
                        .expect("geometry")
                        .iter()
                        .map(|entry| {
                            let rect = &entry["rect"];
                            (
                                entry["window"].as_str().expect("window").to_owned(),
                                (
                                    rect["x"].as_i64().unwrap() as i32,
                                    rect["y"].as_i64().unwrap() as i32,
                                    rect["w"].as_i64().unwrap() as i32,
                                    rect["h"].as_i64().unwrap() as i32,
                                    entry.get("overconstrained")
                                        == Some(&serde_json::Value::Bool(true)),
                                ),
                            )
                        })
                        .collect();
                let want: std::collections::BTreeMap<String, (i32, i32, i32, i32, bool)> = tiles
                    .iter()
                    .map(|(w, x, y, ww, h, f)| ((*w).to_owned(), (*x, *y, *ww, *h, *f)))
                    .collect();
                assert_eq!(got, want, "{reply} focused={focused} reversed={reversed}");
                if let Some(first) = &per_focus {
                    assert_eq!(&got, first, "focused={focused} reversed={reversed}");
                } else {
                    per_focus = Some(got);
                }
                // Focus lands on the focused window's own leaf.
                let focus_leaf = reply["desired_focus"]["leaf"].as_str().expect("focus leaf");
                let focus_entry = reply["desired_geometry"]
                    .as_array()
                    .expect("geometry")
                    .iter()
                    .find(|entry| entry["window"].as_str() == Some(*focused))
                    .expect("focused tile");
                assert_eq!(
                    focus_entry["leaf"].as_str().expect("leaf"),
                    focus_leaf,
                    "{reply}"
                );
                let report = planner.engine.last_adoption_fit().expect("fit logs");
                assert_eq!(report.outcome, "fallback", "{reply}");
                assert_eq!(report.reason, "centre_split", "{reply}");
                assert_eq!(report.centre_splits, 3, "{reply}");
            }
        }
    }

    #[test]
    fn adoption_fit_clean_2x2_stays_fitted_with_identity_order_axes() {
        // Clean 2x2 needs no centre split: adoption stays fitted and
        // preserves window identity, geometry order, and split axes.
        let windows = [
            ("win-1", 0, 0, 600, 400),
            ("win-2", 600, 0, 600, 400),
            ("win-3", 0, 400, 600, 400),
            ("win-4", 600, 400, 600, 400),
        ];
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "adopt-clean2x2-1",
            "owner-1",
            "gen-1",
            "win-4",
            &windows,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2", "win-3", "win-4"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 600, 400)),
                ("win-2".to_owned(), (600, 0, 600, 400)),
                ("win-3".to_owned(), (0, 400, 600, 400)),
                ("win-4".to_owned(), (600, 400, 600, 400)),
            ]),
            "{reply}"
        );
        let report = planner.engine.last_adoption_fit().expect("fit logs");
        assert_eq!(report.outcome, "fitted", "{reply}");
        assert_eq!(report.reason, "ok", "{reply}");
        assert_eq!(report.centre_splits, 0, "{reply}");
        // Same 2x2 with satisfiable native minimums (NP 401x246) stays fitted
        // with identical geometry and no flags.
        let hinted: std::collections::BTreeMap<&str, HintPair> =
            ["win-1", "win-2", "win-3", "win-4"]
                .into_iter()
                .map(|w| (w, (Some((401, 246)), None)))
                .collect();
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request_with_hints(
            "adopt-clean2x2-2",
            "owner-1",
            "gen-1",
            "win-4",
            &windows,
            &hinted,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2", "win-3", "win-4"]);
        assert_eq!(
            geometry_by_window(&reply),
            std::collections::BTreeMap::from([
                ("win-1".to_owned(), (0, 0, 600, 400)),
                ("win-2".to_owned(), (600, 0, 600, 400)),
                ("win-3".to_owned(), (0, 400, 600, 400)),
                ("win-4".to_owned(), (600, 400, 600, 400)),
            ]),
            "{reply}"
        );
        for entry in reply["desired_geometry"].as_array().expect("geometry") {
            assert!(entry.get("overconstrained").is_none(), "{reply}");
            assert!(entry.get("client_clamped").is_none(), "{reply}");
        }
        let report = planner.engine.last_adoption_fit().expect("fit logs");
        assert_eq!(report.outcome, "fitted", "{reply}");
        assert_eq!(report.reason, "ok", "{reply}");
        assert_eq!(report.centre_splits, 0, "{reply}");
    }

    #[test]
    fn adoption_fit_min_infeasible_clean_topology_declines_to_sequential() {
        // Clean side-by-side topology whose carried minimums exceed the
        // extent declines to the same sequential seed (no overconstrained
        // adoption is committed).
        let windows = [("win-1", 0, 0, 600, 800), ("win-2", 600, 0, 600, 800)];
        let hints: std::collections::BTreeMap<&str, HintPair> = [
            ("win-1", (Some((700, 100)), None)),
            ("win-2", (Some((700, 100)), None)),
        ]
        .into_iter()
        .collect();
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request_with_hints(
            "adopt-mininfeas-1",
            "owner-1",
            "gen-1",
            "win-2",
            &windows,
            &hints,
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        assert_geometry_covers(&reply, &["win-1", "win-2"]);
        let report = planner.engine.last_adoption_fit().expect("fit logs");
        assert_eq!(report.outcome, "fallback", "{reply}");
        assert_eq!(report.reason, "min_infeasible", "{reply}");
        assert_eq!(report.centre_splits, 0, "{reply}");
    }

    #[test]
    fn adoption_fit_big_small_declines_to_equal_seed() {
        // Overlapping big/small pairs need a centre split, so they decline
        // to the normal sequential seed: equal long-edge splits, not
        // proportional shares. The focused window seeds last, so the two
        // focus choices mirror each other.
        for (tag, windows, horizontal_axis) in [
            (
                "adopt-sizes-h",
                [
                    ("win-big", 0, 0, 1000, 800),
                    ("win-small", 800, 0, 300, 800),
                ]
                .as_slice(),
                true,
            ),
            (
                "adopt-sizes-v",
                [
                    ("win-big", 0, 0, 1200, 600),
                    ("win-small", 0, 500, 1200, 200),
                ]
                .as_slice(),
                false,
            ),
        ] {
            let mut seen = std::collections::BTreeMap::new();
            for focused in ["win-big", "win-small"] {
                let mut planner = Planner::new();
                let reply = parse_reply(&planner.evaluate(&retained_request(
                    tag,
                    "owner-1",
                    "gen-1",
                    focused,
                    windows,
                    serde_json::json!({"op": "reconcile"}),
                )));
                assert_eq!(reply["outcome"], "planned", "{reply}");
                let got = geometry_by_window(&reply);
                assert_geometry_covers(&reply, &["win-big", "win-small"]);
                let big = got["win-big"];
                let small = got["win-small"];
                if horizontal_axis {
                    assert_eq!((big.2, small.2), (600, 600), "{reply} focused={focused}");
                    assert_eq!(big.2 + small.2, 1200, "{reply}");
                } else {
                    // Wide domain seeds along the long edge (side-by-side),
                    // so the vertically overlapping pair still splits width.
                    assert_eq!((big.2, small.2), (600, 600), "{reply} focused={focused}");
                    assert_eq!((big.3, small.3), (800, 800), "{reply} focused={focused}");
                }
                seen.insert(focused, got);
                let report = planner.engine.last_adoption_fit().expect("fit logs");
                assert_eq!(report.outcome, "fallback", "{reply}");
                assert_eq!(report.reason, "centre_split", "{reply}");
                assert!(report.centre_splits > 0, "{reply}");
            }
            // Focus-last seeding mirrors placement across focus choices.
            assert_ne!(seen["win-big"], seen["win-small"], "{tag}");
        }
    }

    #[test]
    fn adoption_fit_summary_is_correlated_bounded_and_single_per_attempt() {
        let line = summarize_adoption_fit("adopt-log-1", "fitted", 3, "ok", 0);
        assert_eq!(
            line,
            "omnitiler:adoption-fit outcome=fitted windows=3 reason=ok centre_splits=0 correlation=adopt-log-1"
        );
        let fallback = summarize_adoption_fit("adopt-log-2", "fallback", 2, "single_window", 0);
        assert!(fallback.contains("outcome=fallback"), "{fallback}");
        assert!(fallback.contains("windows=2"), "{fallback}");
        assert!(fallback.contains("reason=single_window"), "{fallback}");
        assert!(fallback.contains("correlation=adopt-log-2"), "{fallback}");
        assert!(!fallback.contains("win-1"), "{fallback}");
        let garbage = summarize_adoption_fit("evil!!", "FITTED!!", 2, "evil reason!!", 0);
        assert!(!garbage.contains("evil"), "{garbage}");
        assert!(garbage.contains("correlation=-"), "{garbage}");

        // Fresh single-window and fit-excluded attempts log fallback reasons.
        let mut single = Planner::new();
        let single_reply = parse_reply(&single.evaluate(&retained_request(
            "adopt-log-single-1",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(single_reply["outcome"], "planned", "{single_reply}");
        let single_report = single.engine.last_adoption_fit().expect("single logs");
        assert_eq!(single_report.outcome, "fallback", "{single_reply}");
        assert_eq!(single_report.reason, "single_window", "{single_reply}");
        assert_eq!(
            summarize_adoption_fit(
                single_report.correlation.as_str(),
                single_report.outcome,
                single_report.windows,
                single_report.reason,
                single_report.centre_splits,
            ),
            "omnitiler:adoption-fit outcome=fallback windows=1 reason=single_window centre_splits=0 correlation=adopt-log-single-1"
        );

        let mut excluded = Planner::new();
        let excluded_reply = parse_reply(&excluded.evaluate(&fit_excluded_request(
            "adopt-log-excluded-1",
            "win-2",
            &[("win-1", 0, 0, 400, 800), ("win-2", 400, 0, 800, 800)],
            &["win-1"],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(excluded_reply["outcome"], "planned", "{excluded_reply}");
        let excluded_report = excluded.engine.last_adoption_fit().expect("excluded logs");
        assert_eq!(excluded_report.reason, "fit_excluded", "{excluded_reply}");
    }

    #[test]
    fn placement_trace_summaries_carry_topology_and_anchors() {
        use tiler_core::directional::{Axis, NodeId, WindowId};
        use tiler_core::engine::{EngineSendPlacementTrace, EngineStartupFitTrace, StartupInput};
        use tiler_core::geometry::Rect;
        use tiler_core::ids::CorrelationId;
        let correlation = CorrelationId::parse("place-trace-1").expect("valid");
        let startup = EngineStartupFitTrace {
            correlation: correlation.clone(),
            windows: 2,
            domain_bounds: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            inputs: vec![
                StartupInput {
                    window: WindowId("win-1".to_owned()),
                    rect: Rect {
                        x: 0,
                        y: 0,
                        w: 400,
                        h: 600,
                    },
                },
                StartupInput {
                    window: WindowId("win-2".to_owned()),
                    rect: Rect {
                        x: 400,
                        y: 0,
                        w: 400,
                        h: 600,
                    },
                },
            ],
            outcome: "fitted",
            reason: "ok",
            centre_splits: 0,
            leaves: 2,
            topology: "H[1,1](L,L)".to_owned(),
        };
        let line = summarize_startup_fit_trace(&startup);
        assert!(line.starts_with(PLACEMENT_TRACE_PREFIX), "{line}");
        assert!(line.contains("kind=startup-fit"), "{line}");
        assert!(line.contains("windows=2"), "{line}");
        assert!(line.contains("domain=0,0,800,600"), "{line}");
        assert!(line.contains("win-1=0,0,400,600"), "{line}");
        assert!(line.contains("win-2=400,0,400,600"), "{line}");
        assert!(line.contains("outcome=fitted"), "{line}");
        assert!(line.contains("leaves=2"), "{line}");
        assert!(line.contains("topology=H[1,1](L,L)"), "{line}");
        assert!(line.contains("correlation=place-trace-1"), "{line}");
        let send = EngineSendPlacementTrace {
            correlation,
            anchor_kind: "remembered",
            anchor: Some(NodeId("leaf-1".to_owned())),
            axis: Axis::Horizontal,
            projected: Rect {
                x: 0,
                y: 0,
                w: 400,
                h: 600,
            },
            target_leaves: 2,
        };
        let send_line = summarize_send_placement_trace(&send);
        assert!(send_line.contains("kind=send-placement"), "{send_line}");
        assert!(send_line.contains("anchor=remembered"), "{send_line}");
        assert!(send_line.contains("anchor_leaf=leaf-1"), "{send_line}");
        assert!(send_line.contains("axis=horizontal"), "{send_line}");
        assert!(send_line.contains("projected=0,0,400,600"), "{send_line}");
        // Fresh planner with no op records no placement lines.
        assert!(Planner::new().placement_trace_lines().is_empty());
    }

    #[test]
    fn fresh_reconcile_records_startup_fit_trace() {
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "place-trace-2",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        let trace = planner
            .engine
            .last_startup_fit_trace()
            .expect("startup trace logs");
        assert_eq!(trace.outcome, "fallback");
        assert_eq!(trace.reason, "single_window");
        assert_eq!(trace.windows, 1);
        assert_eq!(trace.inputs.len(), 1);
        assert_eq!(trace.inputs[0].window.0, "win-1");
        assert!(!trace.topology.is_empty(), "{trace:?}");
        let lines = planner.placement_trace_lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("kind=startup-fit"), "{lines:?}");
        assert!(lines[0].contains("correlation=place-trace-2"), "{lines:?}");
        assert!(lines[0].contains("win-1="), "{lines:?}");
    }

    #[test]
    fn seeded_fallback_startup_carries_resulting_tree() {
        // A single window declines the fit (`single_window`) then seeds:
        // the fallback trace must still carry the resulting seeded tree.
        let mut planner = Planner::new();
        let reply = parse_reply(&planner.evaluate(&retained_request(
            "place-trace-3",
            "owner-1",
            "gen-1",
            "win-1",
            &[("win-1", 0, 0, 400, 600)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(reply["outcome"], "planned", "{reply}");
        let trace = planner
            .engine
            .last_startup_fit_trace()
            .expect("startup trace logs");
        assert_eq!(trace.outcome, "fallback", "{reply}");
        assert_eq!(trace.reason, "single_window", "{reply}");
        assert_eq!(trace.leaves, 1, "{reply}");
        assert_eq!(trace.topology, "L", "{reply}");
    }

    #[test]
    fn output_relocation_preserves_topology_on_same_workspace_survivor() {
        // Displaced workspace (same id) observed on a survivor output reuses
        // the retained tree instead of reseeding. Offline only.
        let mut planner = Planner::new();
        let first = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-1",
            "owner-1",
            "gen-1",
            "out-removed",
            "ws-9",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(first["outcome"], "planned", "{first}");
        assert_eq!(planner.retained_domains(), 1);
        // Same workspace id now observed on the survivor with the same
        // members: a reconcile must project the retained allocation, not
        // reseed, and the domain count stays one (source moved, not copied).
        let moved = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-2",
            "owner-1",
            "gen-1",
            "out-survivor",
            "ws-9",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(moved["outcome"], "planned", "{moved}");
        assert_eq!(moved["detail"]["kind"], "reconcile", "{moved}");
        assert_eq!(planner.retained_domains(), 1, "{moved}");
        assert_geometry_covers(&moved, &["win-1", "win-2"]);
    }

    #[test]
    fn output_relocation_returns_with_current_contents_after_edits() {
        // Membership edits while displaced converge through complete-observation
        // reconcile on the relocated tree: moved-out stays out, moved-in
        // reconciles into the relocated topology.
        let mut planner = Planner::new();
        let seed = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-edit-1",
            "owner-1",
            "gen-1",
            "out-old",
            "ws-7",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(seed["outcome"], "planned", "{seed}");
        // Displace with the same set: relocation preserves the tree.
        let displaced = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-edit-2",
            "owner-1",
            "gen-1",
            "out-new",
            "ws-7",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(displaced["outcome"], "planned", "{displaced}");
        assert_eq!(planner.retained_domains(), 1, "{displaced}");
        // While displaced, remove win-2 (moved-out stays out).
        let removed = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-edit-3",
            "owner-1",
            "gen-1",
            "out-new",
            "ws-7",
            "win-1",
            &[("win-1", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(removed["outcome"], "planned", "{removed}");
        // Reconcile win-3 into the displaced workspace (moved-in returns with it).
        let admitted = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-edit-4",
            "owner-1",
            "gen-1",
            "out-new",
            "ws-7",
            "win-3",
            &[("win-1", 0, 0, 100, 80), ("win-3", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(admitted["outcome"], "planned", "{admitted}");
        assert_geometry_covers(&admitted, &["win-1", "win-3"]);
        // Return to the original output with current contents.
        let returned = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-edit-5",
            "owner-1",
            "gen-1",
            "out-old",
            "ws-7",
            "win-1",
            &[("win-1", 0, 0, 100, 80), ("win-3", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(returned["outcome"], "planned", "{returned}");
        assert_geometry_covers(&returned, &["win-1", "win-3"]);
        assert_eq!(planner.retained_domains(), 1, "{returned}");
    }

    #[test]
    fn output_relocation_never_merges_into_survivor_visible_tree() {
        // A survivor output with its own workspace keeps its session; the
        // displaced workspace arrives as a separate domain, never merged.
        let mut planner = Planner::new();
        let survivor = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-merge-1",
            "owner-1",
            "gen-1",
            "out-keep",
            "ws-keep",
            "win-k",
            &[("win-k", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(survivor["outcome"], "planned", "{survivor}");
        let displaced = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-merge-2",
            "owner-1",
            "gen-1",
            "out-gone",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(displaced["outcome"], "planned", "{displaced}");
        assert_eq!(planner.retained_domains(), 2);
        let relocated = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-merge-3",
            "owner-1",
            "gen-1",
            "out-keep",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(relocated["outcome"], "planned", "{relocated}");
        // Two separate domains: survivor visible plus displaced, not one
        // merged tree.
        assert_eq!(planner.retained_domains(), 2, "{relocated}");
        assert_geometry_covers(&relocated, &["win-a"]);
    }

    #[test]
    fn output_relocation_target_collision_is_atomic() {
        // A usable non-empty target is a collision: complete-observation
        // reconcile follows the normal path with no source mutation. Both
        // domains stay retained and usable.
        let mut planner = Planner::new();
        let source = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-coll-1",
            "owner-1",
            "gen-1",
            "out-gone",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(source["outcome"], "planned", "{source}");
        let target = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-coll-2",
            "owner-1",
            "gen-1",
            "out-keep",
            "ws-away",
            "win-k",
            &[("win-k", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(target["outcome"], "planned", "{target}");
        assert_eq!(planner.retained_domains(), 2);
        // Reconcile on the existing target follows the normal path; the
        // source is untouched (no relocation, no destruction).
        let again = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-coll-3",
            "owner-1",
            "gen-1",
            "out-keep",
            "ws-away",
            "win-k",
            &[("win-k", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(again["outcome"], "planned", "{again}");
        assert_eq!(planner.retained_domains(), 2, "{again}");
        let source_again = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-coll-4",
            "owner-1",
            "gen-1",
            "out-gone",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(source_again["outcome"], "planned", "{source_again}");
        assert_eq!(planner.retained_domains(), 2, "{source_again}");
    }

    #[test]
    fn output_relocation_mismatched_target_does_not_relocate_source() {
        // A target session that existed at handling start is never removed
        // or superseded for relocation, even when the carried bounds skew.
        // The stale target follows the normal complete-observation reconcile
        // path while the source stays retained and usable.
        let mut planner = Planner::new();
        let source = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-mismatch-1",
            "owner-1",
            "gen-1",
            "out-gone",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(source["outcome"], "planned", "{source}");
        let target = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-mismatch-2",
            "owner-1",
            "gen-1",
            "out-keep",
            "ws-away",
            "win-k",
            &[("win-k", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(target["outcome"], "planned", "{target}");
        assert_eq!(planner.retained_domains(), 2);
        // Reconcile on the existing target with different bounds: the
        // retained target reprojects/converges through the normal path, but
        // source relocation must not run because the target existed at
        // handling start.
        let mut mismatched: serde_json::Value = serde_json::from_str(&retained_request_for_domain(
            "reloc-mismatch-3",
            "owner-1",
            "gen-1",
            "out-keep",
            "ws-away",
            "win-k2",
            &[("win-k", 0, 0, 100, 80), ("win-k2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("request JSON");
        mismatched["domain"]["bounds"] = serde_json::json!({"x": 0, "y": 0, "w": 800, "h": 600});
        let reseeded = parse_reply(&planner.evaluate(&mismatched.to_string()));
        assert_eq!(reseeded["outcome"], "planned", "{reseeded}");
        // Normal reconcile kept the target; the source was not relocated.
        assert_eq!(planner.retained_domains(), 2, "{reseeded}");
        let source_key = DomainKey {
            output: OutputId("out-gone".to_owned()),
            workspace: WorkspaceId("ws-away".to_owned()),
        };
        let target_key = DomainKey {
            output: OutputId("out-keep".to_owned()),
            workspace: WorkspaceId("ws-away".to_owned()),
        };
        assert!(planner.engine.contains(&source_key), "{reseeded}");
        assert!(planner.engine.contains(&target_key), "{reseeded}");
        assert_geometry_covers(&reseeded, &["win-k", "win-k2"]);
        // Source is untouched and still reconciles on its own domain.
        let source_again = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-mismatch-4",
            "owner-1",
            "gen-1",
            "out-gone",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(source_again["outcome"], "planned", "{source_again}");
        assert_eq!(planner.retained_domains(), 2, "{source_again}");
    }

    #[test]
    fn output_relocation_outer_gap_mismatch_is_atomic() {
        // Outer-gap mismatch against the source fails closed with no retained
        // mutation: the source stays and no target is created. Complete
        // observation carries the exact source/target ids.
        let mut planner = Planner::new();
        let seed = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-gap-1",
            "owner-1",
            "gen-1",
            "out-gone",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(seed["outcome"], "planned", "{seed}");
        assert_eq!(planner.retained_domains(), 1);
        let mut mismatched: serde_json::Value = serde_json::from_str(&retained_request_for_domain(
            "reloc-gap-2",
            "owner-1",
            "gen-1",
            "out-keep",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("request JSON");
        mismatched["domain"]["outer_gap"] = serde_json::json!(8);
        let rejected_reply = parse_reply(&planner.evaluate(&mismatched.to_string()));
        assert_eq!(rejected_reply["outcome"], "rejected", "{rejected_reply}");
        assert_eq!(planner.retained_domains(), 1, "{rejected_reply}");
        // Source is untouched and still reconciles.
        let source_again = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-gap-3",
            "owner-1",
            "gen-1",
            "out-gone",
            "ws-away",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(source_again["outcome"], "planned", "{source_again}");
        assert_eq!(planner.retained_domains(), 1, "{source_again}");
    }

    #[test]
    fn output_relocation_ambiguous_source_is_atomic() {
        // Two usable sources with the same workspace never relocate: with no
        // pending, a fresh reconcile seeds the new domain through the same
        // admission route while both sources stay retained (ambiguous-source
        // fresh-seed rule).
        let mut planner = Planner::new();
        for (correlation, output, window) in [
            ("reloc-amb-1", "out-a", "win-a"),
            ("reloc-amb-2", "out-b", "win-b"),
        ] {
            let seeded = parse_reply(&planner.evaluate(&retained_request_for_domain(
                correlation,
                "owner-1",
                "gen-1",
                output,
                "ws-x",
                window,
                &[(window, 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(seeded["outcome"], "planned", "{seeded}");
        }
        assert_eq!(planner.retained_domains(), 2);
        let seeded = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-amb-3",
            "owner-1",
            "gen-1",
            "out-c",
            "ws-x",
            "win-a",
            &[("win-a", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(seeded["outcome"], "planned", "{seeded}");
        assert_geometry_covers(&seeded, &["win-a"]);
        assert_eq!(planner.retained_domains(), 3, "{seeded}");
        // Neither source moved: both still reconcile their own members.
        for (correlation, output, window) in [
            ("reloc-amb-4", "out-a", "win-a"),
            ("reloc-amb-5", "out-b", "win-b"),
        ] {
            let source = parse_reply(&planner.evaluate(&retained_request_for_domain(
                correlation,
                "owner-1",
                "gen-1",
                output,
                "ws-x",
                window,
                &[(window, 0, 0, 100, 80)],
                serde_json::json!({"op": "reconcile"}),
            )));
            assert_eq!(source["outcome"], "planned", "{source}");
            assert_geometry_covers(&source, &[window]);
        }
    }

    #[test]
    fn output_relocation_preserves_exception_class_and_float_geometry() {
        // A floated exception keeps its class (flags) and floating geometry
        // across relocation; only the homing output moves. Seed and relocate
        // through complete-observation reconcile (no admit newcomer on the
        // move); a later complete reconcile admits win-3. Revision is
        // preserved across the move and advances once for the admit.
        let mut planner = Planner::new();
        let seed = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-exc-1",
            "owner-1",
            "gen-1",
            "out-removed",
            "ws-9",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(seed["outcome"], "planned", "{seed}");
        let floated = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-exc-2",
            "owner-1",
            "gen-1",
            "out-removed",
            "ws-9",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "toggle-float", "window": "win-1"}),
        )));
        assert_eq!(floated["outcome"], "planned", "{floated}");
        let source_key = DomainKey {
            output: OutputId("out-removed".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
        };
        let before = planner
            .engine
            .session(&source_key)
            .expect("source retained")
            .clone();
        assert_eq!(before.exception_count(), 1, "{floated}");
        let before_revision = before.accepted_revision();
        let before_exception = before
            .exception_observed()
            .into_iter()
            .next()
            .expect("float exception");
        assert!(before_exception.floating, "{floated}");
        let before_float_geometry = before.floating_geometry(&before_exception.window);
        // Relocated exact overlapping observation (no newcomer) reconciles on
        // the moved tree: the floated window stays flagged floating.
        let mut displaced: serde_json::Value = serde_json::from_str(&retained_request_for_domain(
            "reloc-exc-3",
            "owner-1",
            "gen-1",
            "out-survivor",
            "ws-9",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("request JSON");
        displaced["windows"][0]["floating"] = serde_json::json!(true);
        let moved = parse_reply(&planner.evaluate(&displaced.to_string()));
        assert_eq!(moved["outcome"], "planned", "{moved}");
        assert_eq!(moved["detail"]["kind"], "reconcile", "{moved}");
        assert_eq!(planner.retained_domains(), 1, "{moved}");
        let target_key = DomainKey {
            output: OutputId("out-survivor".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
        };
        let relocated = planner
            .engine
            .session(&target_key)
            .expect("target retained");
        // Relocation itself adds zero: revision preserved.
        assert_eq!(relocated.accepted_revision(), before_revision, "{moved}");
        assert_eq!(relocated.exception_count(), 1, "{moved}");
        let relocated_exception = relocated
            .exception_observed()
            .into_iter()
            .next()
            .expect("relocated exception");
        assert_eq!(
            relocated_exception.floating, before_exception.floating,
            "{moved}"
        );
        assert_eq!(
            relocated_exception.fullscreen, before_exception.fullscreen,
            "{moved}"
        );
        assert_eq!(
            relocated_exception.maximized, before_exception.maximized,
            "{moved}"
        );
        assert_eq!(
            relocated_exception.sticky, before_exception.sticky,
            "{moved}"
        );
        assert_eq!(relocated_exception.output.0, "out-survivor", "{moved}");
        assert_eq!(relocated_exception.workspace.0, "ws-9", "{moved}");
        assert_eq!(
            relocated.floating_geometry(&relocated_exception.window),
            before_float_geometry,
            "{moved}"
        );
        // Separate retained complete reconcile admits win-3 on the relocated
        // tree, preserving topology and exception class; revision advances
        // exactly once.
        let mut admit: serde_json::Value = serde_json::from_str(&retained_request_for_domain(
            "reloc-exc-4",
            "owner-1",
            "gen-1",
            "out-survivor",
            "ws-9",
            "win-3",
            &[
                ("win-1", 0, 0, 100, 80),
                ("win-2", 0, 0, 100, 80),
                ("win-3", 0, 0, 100, 80),
            ],
            serde_json::json!({"op": "reconcile"}),
        ))
        .expect("request JSON");
        admit["windows"][0]["floating"] = serde_json::json!(true);
        let admitted = parse_reply(&planner.evaluate(&admit.to_string()));
        assert_eq!(admitted["outcome"], "planned", "{admitted}");
        assert_eq!(admitted["detail"]["kind"], "reconcile", "{admitted}");
        assert_geometry_covers(&admitted, &["win-2", "win-3"]);
        assert_eq!(planner.retained_domains(), 1, "{admitted}");
        let after = planner
            .engine
            .session(&target_key)
            .expect("target retained");
        assert_eq!(after.accepted_revision(), before_revision + 1, "{admitted}");
        assert_eq!(after.exception_count(), 1, "{admitted}");
        let after_exception = after
            .exception_observed()
            .into_iter()
            .next()
            .expect("admitted exception");
        assert_eq!(
            after_exception.floating, before_exception.floating,
            "{admitted}"
        );
        assert_eq!(
            after_exception.fullscreen, before_exception.fullscreen,
            "{admitted}"
        );
        assert_eq!(
            after_exception.maximized, before_exception.maximized,
            "{admitted}"
        );
        assert_eq!(
            after_exception.sticky, before_exception.sticky,
            "{admitted}"
        );
        assert_eq!(after_exception.output.0, "out-survivor", "{admitted}");
        assert_eq!(after_exception.workspace.0, "ws-9", "{admitted}");
        assert_eq!(
            after.floating_geometry(&after_exception.window),
            before_float_geometry,
            "{admitted}"
        );
    }

    #[test]
    fn output_relocation_preserves_revision_gap_tree_and_focus() {
        // Successful relocation keeps accepted revision, inner gap, topology
        // shares, window homing, and focus; only output identity changes.
        let mut planner = Planner::new();
        let seed = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-state-1",
            "owner-1",
            "gen-1",
            "out-removed",
            "ws-9",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(seed["outcome"], "planned", "{seed}");
        let source_key = DomainKey {
            output: OutputId("out-removed".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
        };
        let before = planner
            .engine
            .session(&source_key)
            .expect("source retained")
            .clone();
        let before_revision = before.accepted_revision();
        let before_snapshot = before.snapshot();
        let (before_focus_domain, before_focus_leaf) = before.focus();
        assert!(before_focus_leaf.is_some(), "seeded focus");
        let moved = parse_reply(&planner.evaluate(&retained_request_for_domain(
            "reloc-state-2",
            "owner-1",
            "gen-1",
            "out-survivor",
            "ws-9",
            "win-2",
            &[("win-1", 0, 0, 100, 80), ("win-2", 0, 0, 100, 80)],
            serde_json::json!({"op": "reconcile"}),
        )));
        assert_eq!(moved["outcome"], "planned", "{moved}");
        assert_eq!(planner.retained_domains(), 1, "{moved}");
        assert!(!planner.engine.contains(&source_key), "{moved}");
        let target_key = DomainKey {
            output: OutputId("out-survivor".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
        };
        let after = planner
            .engine
            .session(&target_key)
            .expect("target retained");
        assert_eq!(after.accepted_revision(), before_revision, "{moved}");
        // Topology shares and window set are preserved; only output homing
        // moves to the survivor.
        let after_snapshot = after.snapshot();
        assert_eq!(
            after_snapshot.windows.len(),
            before_snapshot.windows.len(),
            "{moved}"
        );
        for link in &after_snapshot.windows {
            assert_eq!(link.output.0, "out-survivor", "{moved}");
            assert_eq!(link.workspace.0, "ws-9", "{moved}");
        }
        let (after_focus_domain, after_focus_leaf) = after.focus();
        assert_eq!(after_focus_domain, Some(target_key.clone()), "{moved}");
        assert_eq!(after_focus_leaf, before_focus_leaf, "{moved}");
        assert_eq!(before_focus_domain, Some(source_key), "{moved}");
    }
}
