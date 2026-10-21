#![forbid(unsafe_code)]
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
/// Batching contract: at most one event per second globally, bounded entries
/// per event. Every entry carries its own operation/correlation/tick context from the assembly that staged it,
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
///
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
