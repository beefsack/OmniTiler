#![forbid(unsafe_code)]

//! Development-only guarded command harness for the signed live fixture.
//!
//! Narrow sibling of [`crate::owned_control::run_owned_post`] and
//! [`crate::textedit_post::run_textedit_post`]: delivers fixed semantic
//! product commands through the REAL product input ingress (the untagged
//! session-stream tap the product tap consumes like user chords, via the
//! shared [`crate::synth::raw_post_key_event_to_session`] primitive,
//! truthfully synthetic, never physical, never a direct core/adapter
//! command injection) to windows owned by the signed fixture bundle. Native
//! latency legs and the later shared-spec stress runner drive product
//! select/send and directional focus/move through this route.
//!
//! Ownership contract (explicit, independently reviewable). Signing
//! identity is proven ONCE, at session bind; every input is preceded by a
//! cheap final commit check, and no blocking authentication sits between the
//! balanced down/up pair:
//!
//! - Bind (once per driver, outside any held key): session gates (SLPS
//!   symbols, AX trust, Secure Input refused unless observed inactive -
//!   unknown refuses here, unlocked session, post access - same legs as the
//!   helper/TextEdit routes but strict on Secure Input), then live
//!   bundle/executable/launch reads plus `codesign --verify` and the
//!   designated-requirement read against the expected fixture requirement
//!   passed as `--signer` (captured by the leg driver from `fixture-verify`
//!   output at preflight; machine-specific, never hardcoded here). Ad-hoc
//!   or foreign-signed binaries refuse even when they carry the fixture
//!   bundle identifier. Each `codesign` child runs under a genuine
//!   deadline: past it the child is killed and reaped (no orphan, no
//!   zombie) and the bind refuses. No LaunchServices launch happens here
//!   or per action (a future in-process burst driver reuses the bound
//!   session with its own pacing).
//! - Full proof (before the down edge, nothing held): session gates, then
//!   a nonce-gated read-only `get` over the announced Unix socket (the
//!   announced socket path is used and shape-checked; caller string
//!   equality with the announcement is NOT authentication), the live
//!   bundle/executable/launch continuity against the bound values, the CG
//!   owner join plus the sentinel-title caller join cross-checked against
//!   the LIVE server snapshot (the native title's run tag must equal the
//!   socket snapshot's run tag), and the AX window join for the exact
//!   `wid`. The first full proof for a `(caller, wid)` binds that window's
//!   incarnation (run tag plus the opaque server `native_handle`, retained
//!   and demanded stable across calls - never parsed, never compared to a
//!   CGWID); a re-created caller window with a fresh handle refuses as
//!   incarnation drift. No frontmost/oracle/subprocess read sits after the
//!   target reads here: ordering is socket, identity, CG, AX, and only
//!   then the commit check below.
//! - Commit check (immediately before EACH post, including the up edge
//!   while the key is held): session gates plus process continuity
//!   (executable/launch still equal the bound values, no fork, no socket)
//!   plus the fresh CG owner/title/run-tag join for the exact `wid`, plus
//!   (on the post path) the fresh frontmost PID read LAST, equal to the
//!   fixture PID. Any foreign frontmost, including the hosting terminal,
//!   refuses. The focus path commits everything except frontmost so it can
//!   ACQUIRE foreground; it proves the rest, then dispatches SLPS and
//!   awaits agreement. The commit performs no socket exchange, no AX
//!   enumeration, and no subprocess: while a key is held only this cheap
//!   check runs, and on its failure the held key is released
//!   unconditionally through exactly one gateless cleared-flags key-up.
//! - Selected-oracle agreement for exactly `(pid, wid)` is proven by its
//!   own step after every blocking full read completes and immediately
//!   before the cheap down commit (down path only, never on the
//!   focus-acquisition path, never while a key is held). A sample taken
//!   before the up-to-15 s AX inventory could name a window the inventory
//!   outlasts; sampling here keeps exact selected attribution fresh. The
//!   commit's fresh frontmost read stays the last-moment foreground
//!   authority for both edges, and the up edge is release continuity
//!   (cheap target/session/foreground recheck plus unconditional cleared
//!   release on loss) - never a second attribution: genuine product focus
//!   movement after the down post must not strand a held key.
//! - Process lifetime: live bundle plus executable plus LaunchServices
//!   launch date must equal the bound session values at bind, at every
//!   full proof, and at every commit (PID reuse or relaunch refuses).
//!   Direct-binary launches carry no bundle identity and refuse at the
//!   bundle gate.
//!
//! Anything mismatched, stale, or unreadable refuses before any post.
//! Outputs are metadata-only JSON (IDs, stages, latencies - never titles,
//! content, paths, nonces, executables, or requirement text).
//!
//! Chord vocabulary (fixed, product-armed only - no new hotkeys, no
//! arbitrary keycodes): the directional focus/move letters and left/right
//! arrows resolve through the single owned-post home
//! ([`crate::owned_control::resolve_chord`]); numbered selects `select-0..9`
//! and send-follows `send-0..9` carry the Desktop-N digit keycode with the
//! product arms (trailing `0` is Desktop 10, armed end to end); the
//! relative/previous selects `rel-prev`/`rel-next`/`prev-toggle` resolve
//! through the single workspace catalogue with the exact product flags.
//! Stay sends have no default product chord (bindable-unbound), relative
//! sends/stays and trailing stay-0 are unimplemented catalogue rows, and the
//! TextEdit-only transports (`escape`, `stay-*`) are never fixture routes:
//! all refuse as unknown/unsupported rather than riding a substitute chord.
//!
//! Unconditional cleared-modifier release: once the down edge has posted, an
//! [`EdgeGuard`](crate::textedit_post::EdgeGuard) owns the pending key (the
//! same narrow reused primitive as the TextEdit route, with its `Drop`
//! unwind backstop). Every return path after a posted down releases
//! explicitly gateless with cleared flags; handled SIGINT/SIGTERM reuse the
//! shared diagnostic interrupt arm checked at explicit sequence points. No
//! settle sleep sits between the edges and none is added between commands:
//! a `none` delay means no intentionally inserted delay (guard work still
//! costs CPU and is reported separately as proof microseconds beside the
//! dispatch microseconds, never conflated with them).
//!
//! Reusable driver API: [`FixtureDriver::bind`] verifies the signing
//! identity once and builds a server-level session (no launching, no
//! subprocess per action, no intentional pacing); [`FixtureDriver::post`]
//! and [`FixtureDriver::focus`] drive one target window each, with a full
//! proof outside any held key and a cheap commit immediately before each
//! input. A future in-process sequential/burst driver reuses this type
//! with its own pacing; no stress runner is built here. Guard microseconds
//! report beside dispatch microseconds (proof cost vs delivery cost, never
//! conflated, never claimed zero).
//!
//! Diagnostic route only: reachable solely through the `fixture-post` /
//! `fixture-focus` CLI commands. No product path and no parked scaffold
//! module references this module (regression-tested); this module depends
//! on no parked scaffold either (harness-owned announcement parsing and
//! bundle scope, so it compiles and gates independently of it).

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;

use crate::config::ProbeModifier;
use crate::live_leg::LegAdmission;
use crate::runtime_types::TileBinding;

/// Exact fixture bundle scope for fixture-only legs. Harness-owned (no
/// parked-scaffold dependency): drift against the real fixture bundle fails
/// the bundle gate closed, never mis-authorizes.
pub(crate) const FIXTURE_BUNDLE: &str = "io.github.omnitiler.live-fixture";

/// Server-controlled fixture title sentinel (see `tiler-live-fixture`
/// `protocol::TITLE_SENTINEL`). The full live title is
/// `sentinel:<caller-id>:<run-tag>[ suffix]`; the suffix is test-useful text
/// only. Kept as a local constant (no new crate dependency): drift fails
/// the title gate closed, never mis-authorizes.
pub(crate) const FIXTURE_TITLE_SENTINEL: &str = "OmniTilerTestWindow";

/// Request id used on the harness's own socket connections (one request per
/// connection, so a constant is unambiguous there).
const SOCKET_REQUEST_ID: &str = "fixture-harness-get";

/// Request id for the harness's own window-spawn exchanges (one connection
/// per call, so constants stay unambiguous there).
const SOCKET_SPAWN_REQUEST_ID: &str = "fixture-harness-spawn";

/// Request id for the harness's own control exchanges that close owned
/// fixture windows (one connection per call, so constants stay
/// unambiguous there).
const SOCKET_CLOSE_REQUEST_ID: &str = "fixture-harness-close";

/// Request id for the harness's own server-shutdown exchange.
const SOCKET_SHUTDOWN_REQUEST_ID: &str = "fixture-harness-shutdown";

/// Spawn caller windows on a leg-owned fixture server: one nonce-gated
/// `spawn` exchange with the exact fixture wire shape. The reply carries
/// per-window entries for the caller to judge; a transport refusal fails
/// closed. Unix-only like every other fixture exchange.
pub fn send_fixture_spawn(
    socket_path: &str,
    nonce: &str,
    ids: &[&str],
) -> Result<serde_json::Value, String> {
    if ids.is_empty() {
        return Err("spawn: no caller ids; refusing an empty spawn".to_owned());
    }
    for id in ids {
        if !caller_id_valid(id) {
            return Err("spawn: caller id malformed; refusing".to_owned());
        }
    }
    let windows: Vec<serde_json::Value> =
        ids.iter().map(|id| serde_json::json!({"id": id})).collect();
    let request = serde_json::json!({
        "v": 1,
        "request_id": SOCKET_SPAWN_REQUEST_ID,
        "nonce": nonce,
        "op": "spawn",
        "windows": windows,
    });
    #[cfg(unix)]
    {
        socket_round_trip(socket_path, &request, SOCKET_SPAWN_REQUEST_ID)
    }
    #[cfg(not(unix))]
    {
        let _ = (socket_path, request);
        Err("socket: fixture transport is Unix-only".to_owned())
    }
}

/// Live server snapshot for one caller over a fresh nonce-gated `get`
/// exchange: the responding PID plus the server-controlled title join plus
/// the opaque incarnation handle. Used by leg-owned joins; the driver path
/// uses its own exchange plus [`check_full_reads`].
pub(crate) fn socket_snapshot_for(
    socket_path: &str,
    nonce: &str,
    caller: &str,
) -> Result<SocketSnapshot, String> {
    #[cfg(unix)]
    {
        let reply = socket_get_reply(socket_path, nonce, caller)?;
        parse_socket_get_reply(&reply, SOCKET_REQUEST_ID, caller)
    }
    #[cfg(not(unix))]
    {
        let _ = (socket_path, nonce, caller);
        Err("socket: fixture transport is Unix-only".to_owned())
    }
}
/// one nonce-gated `close` exchange, matched reply required. Per-window
/// outcomes ride the reply for the caller to judge; a transport refusal
/// fails closed. Unix-only like every other fixture exchange.
/// Close exactly the named caller windows on a leg-owned fixture server:
/// one nonce-gated `close` exchange, matched reply required. Per-window
/// outcomes ride the reply for the caller to judge; a transport refusal
/// fails closed. Unix-only like every other fixture exchange.
pub fn send_fixture_close(
    socket_path: &str,
    nonce: &str,
    ids: &[&str],
) -> Result<serde_json::Value, String> {
    if ids.is_empty() {
        return Err("close: no caller ids; refusing an empty close".to_owned());
    }
    for id in ids {
        if !caller_id_valid(id) {
            return Err("close: caller id malformed; refusing".to_owned());
        }
    }
    let request = serde_json::json!({
        "v": 1,
        "request_id": SOCKET_CLOSE_REQUEST_ID,
        "nonce": nonce,
        "op": "close",
        "ids": ids,
    });
    #[cfg(unix)]
    {
        socket_round_trip(socket_path, &request, SOCKET_CLOSE_REQUEST_ID)
    }
    #[cfg(not(unix))]
    {
        let _ = (socket_path, request);
        Err("socket: fixture transport is Unix-only".to_owned())
    }
}

/// Shut down a leg-owned fixture server: one nonce-gated `shutdown`
/// exchange, matched reply required. The server closes its windows, removes
/// its owned socket, and exits; the caller verifies the reap separately.
/// Unix-only like every other fixture exchange.
pub fn send_fixture_shutdown(socket_path: &str, nonce: &str) -> Result<serde_json::Value, String> {
    let request = serde_json::json!({
        "v": 1,
        "request_id": SOCKET_SHUTDOWN_REQUEST_ID,
        "nonce": nonce,
        "op": "shutdown",
    });
    #[cfg(unix)]
    {
        socket_round_trip(socket_path, &request, SOCKET_SHUTDOWN_REQUEST_ID)
    }
    #[cfg(not(unix))]
    {
        let _ = (socket_path, request);
        Err("socket: fixture transport is Unix-only".to_owned())
    }
}

/// Absolute deadline for one socket exchange (connect through matched
/// reply). Genuine deadline: a wedged server must not hang the harness.
const SOCKET_DEADLINE: Duration = Duration::from_secs(5);

/// Per-operation socket timeout inside the absolute deadline.
const SOCKET_OP_TIMEOUT: Duration = Duration::from_secs(2);

/// Bound for one `codesign` invocation (validity plus requirement read).
/// Genuine deadline: the child is abandoned (never waited on blindly) past
/// it and the edge refuses.
const CODESIGN_TIMEOUT: Duration = Duration::from_secs(10);

/// Fixture caller-id shape (mirrors the fixture protocol `validate_id`:
/// `[A-Za-z0-9_.-]+`, 1..=64 bytes). Pure; unit-tested.
pub(crate) fn caller_id_valid(id: &str) -> bool {
    let len = id.len();
    if !(1..=64).contains(&len) {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Parsed fixture `serve` announcement (the single stdout line, the only
/// nonce source). Harness-owned shape (no parked-scaffold dependency):
/// socket path plus nonzero PID plus 64 hex-char nonce. Pure; unit-tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FixtureAnnouncement {
    pub socket: String,
    pub pid: u32,
    pub nonce: String,
}

/// Parse and shape-check one announcement line. Refuses before any use.
pub(crate) fn parse_fixture_announcement(line: &str) -> Result<FixtureAnnouncement, String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("announcement is not JSON: {e}"))?;
    let socket = value
        .get("socket")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    socket_path_valid(socket).map_err(|e| format!("announcement socket invalid: {e}"))?;
    let pid = value
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if pid == 0 || pid > u64::from(u32::MAX) {
        return Err("announcement pid is missing or out of range".to_owned());
    }
    let nonce = value
        .get("nonce")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if nonce.len() != 64 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("announcement nonce is not 64 hex chars".to_owned());
    }
    Ok(FixtureAnnouncement {
        socket: socket.to_owned(),
        pid: pid as u32,
        nonce: nonce.to_owned(),
    })
}

/// Pure Unix-socket path shape (no filesystem touch): nonempty, NUL-free,
/// within the Unix-domain path limit. The directory ownership/mode is the
/// leg setup's job; this only keeps malformed paths from reaching connect.
pub(crate) fn socket_path_valid(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("socket path is missing or empty".to_owned());
    }
    if path.contains('\0') {
        return Err("socket path carries NUL".to_owned());
    }
    if path.len() > 104 {
        return Err("socket path exceeds the Unix-domain limit".to_owned());
    }
    Ok(())
}

/// One fixed fixture chord: catalogue key code plus the Shift/Alt/Aux arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FixtureChord {
    pub key_code: u32,
    pub shift: bool,
    pub alt: bool,
    pub aux: bool,
    pub action: &'static str,
}

/// Resolve a `fixture-post` chord name: the directional focus/move subset of
/// the owned-post vocabulary (letters plus left/right arrows - float, grow,
/// shrink, orientation, and the numbered helper selects/sends are NOT
/// fixture routes) plus the numbered fixture selects `select-0..9` and
/// send-follows `send-0..9` (Desktop-N digit keycode with the product arms;
/// trailing `0` is Desktop 10). The bound relative/previous names are NOT
/// admitted here: their only admission is [`resolve_fixture_bound`]. Stay
/// chords, the TextEdit-only transports, and every other name refuse. Pure;
/// unit-tested.
#[must_use]
pub(crate) fn resolve_fixture_chord(name: &str) -> Option<FixtureChord> {
    // Directional subset of the single owned-post home (no new keycodes,
    // no new arms: the tap consumes these exactly like the helper posts).
    // Helper-only or TextEdit-only names fall through to the numbered legs
    // below, which admit only the fixture selects/sends; anything else
    // still refuses.
    if let Some(chord) = crate::owned_control::resolve_chord(name) {
        let directional = matches!(
            name,
            "focus-h"
                | "focus-j"
                | "focus-k"
                | "focus-l"
                | "focus-left"
                | "focus-right"
                | "move-h"
                | "move-j"
                | "move-k"
                | "move-l"
                | "move-left"
                | "move-right"
        );
        if directional {
            return Some(FixtureChord {
                key_code: chord.key_code,
                shift: chord.shift,
                alt: chord.alt,
                aux: false,
                action: chord.action,
            });
        }
    }
    // Numbered fixture selects/send-follows with the product arms (no arms
    // for the select, Shift for the send-follow). Trailing `0` addresses
    // Desktop 10, matching the armed product trailing rows.
    if let Some(digits) = name
        .strip_prefix("select-")
        .map(|rest| (rest, false))
        .or_else(|| name.strip_prefix("send-").map(|rest| (rest, true)))
    {
        let (rest, shift) = digits;
        if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
            let index: u32 = u32::from(rest.as_bytes()[0] - b'0');
            let desktop = if index == 0 { 10 } else { index };
            return crate::desktop_shortcuts::desktop_digit_keycode(desktop).map(|key_code| {
                FixtureChord {
                    key_code,
                    shift,
                    alt: false,
                    aux: false,
                    action: if shift { "send" } else { "select" },
                }
            });
        }
    }
    None
}

/// Fixture relative/previous select transport for the armed product
/// relative/previous rows: `rel-prev` (previous via H), `rel-next` (next via
/// J), and `prev-toggle` (the previous toggle via Tab). Resolves through the
/// single catalogue home ([`crate::workspace_input::workspace_catalogue`])
/// with the same admission gates as the TextEdit bound leg (implemented row
/// whose action is relative/previous). The posted flags compose
/// [`crate::workspace_input::effective_flags_for`] identically, so the tap
/// consumes them exactly like the user's chords. Never a product arm: the
/// relative sends/stays and outputs are never admitted here. Pure;
/// unit-tested.
pub(crate) fn resolve_fixture_bound(name: &str) -> Option<TileBinding> {
    let id = match name {
        "rel-prev" => "workspace-prev-h",
        "rel-next" => "workspace-next-j",
        "prev-toggle" => "workspace-previous",
        _ => return None,
    };
    let row = crate::workspace_input::workspace_catalogue()
        .into_iter()
        .find(|row| row.id == id)?;
    if !row.implemented {
        return None;
    }
    let binding = row.binding()?;
    match binding.action {
        crate::runtime_types::InputSemanticAction::WorkspaceRelative { .. }
        | crate::runtime_types::InputSemanticAction::WorkspacePrevious => Some(binding),
        _ => None,
    }
}

/// Compose the posted flags exactly like the helper owned-post path: the
/// Ctrl role plus the chord Shift/Alt arms, plus the auxiliary arm for the
/// bound relative/previous chords (which carry `aux`). Shared vocabulary
/// chords carry no `aux`, so they post bit-identically to the helper route.
fn fixture_flags(modifier: ProbeModifier, chord: FixtureChord) -> u64 {
    let (mut want_flags, _) = crate::input::cg_flag_bits(modifier);
    if chord.shift {
        want_flags |= crate::native_input::CG_SHIFT_BIT;
    }
    if chord.alt {
        want_flags |= crate::native_input::CG_ALT_BIT;
    }
    if chord.aux {
        want_flags |= crate::input_policy::aux_flag_for(modifier);
    }
    want_flags
}

/// Pure bundle verdict over plain data (no OS touch): admits exactly the
/// signed fixture bundle. Ghostty is named in its refusal; protected
/// bundles, every other bundle, and unknown/empty bundles refuse (a
/// direct-binary fixture launch carries no bundle identity and refuses
/// here). Signature proof itself rides [`verify_exe_signature`]; this gate
/// never substitutes for it. Unit-tested.
pub(crate) fn bundle_verdict(bundle: Option<&str>) -> Result<(), String> {
    match bundle {
        Some(id) if id == FIXTURE_BUNDLE => Ok(()),
        Some(id) if crate::scope::is_ghostty_bundle(id) => {
            Err("refusing fixture post to Ghostty (never a target)".to_owned())
        }
        Some(id) if crate::ownership::is_protected_bundle(id) => {
            Err("refusing fixture post to a protected bundle (never a target)".to_owned())
        }
        Some(id) => Err(format!(
            "refusing fixture post: live bundle '{id}' is not the signed fixture scope"
        )),
        None => Err(
            "refusing fixture post with unreadable bundle (direct-binary launches carry no bundle identity)"
                .to_owned(),
        ),
    }
}

/// Pure frontmost verdict over plain data (no OS touch): the frontmost PID
/// must be exactly the fixture PID. Any foreign frontmost - including the
/// hosting terminal - refuses before any post. Unit-tested.
pub(crate) fn front_verdict(front_pid: Option<i32>, want_pid: u32) -> Result<(), String> {
    let want = i32::try_from(want_pid)
        .map_err(|_| format!("refusing fixture post: pid {want_pid} out of range"))?;
    match front_pid {
        Some(pid) if pid == want => Ok(()),
        Some(pid) => Err(format!(
            "refusing fixture post: frontmost pid {pid} != fixture pid {want_pid}; refusing foreign frontmost (never the hosting terminal)"
        )),
        None => Err("refusing fixture post with unreadable frontmost app".to_owned()),
    }
}

/// Parse a live fixture window title into `(caller-id, run-tag)`. The title
/// must carry the server-controlled `sentinel:<caller>:<run-tag>` prefix; a
/// caller suffix after a space is accepted and ignored. Anything else
/// (foreign window, malformed title, unknown caller shape) refuses. Pure;
/// unit-tested.
pub(crate) fn parse_fixture_title(title: &str) -> Result<(String, String), String> {
    let rest = title
        .strip_prefix(FIXTURE_TITLE_SENTINEL)
        .and_then(|rest| rest.strip_prefix(':'))
        .ok_or_else(|| {
            "refusing fixture post: window title lacks the fixture sentinel".to_owned()
        })?;
    let (caller, tail) = rest.split_once(':').ok_or_else(|| {
        "refusing fixture post: window title lacks the caller/run-tag join".to_owned()
    })?;
    if !caller_id_valid(caller) {
        return Err("refusing fixture post: window title caller is malformed".to_owned());
    }
    let run_tag = tail.split(' ').next().unwrap_or("");
    if run_tag.is_empty() {
        return Err("refusing fixture post: window title lacks the run tag".to_owned());
    }
    Ok((caller.to_owned(), run_tag.to_owned()))
}

/// Pure run-tag verdict over plain data: the native title's run tag must
/// equal the live server snapshot's run tag (same window incarnation on
/// both sides of the join). Values never travel into the error. Pure;
/// unit-tested.
pub(crate) fn run_tag_verdict(native_tag: &str, socket_tag: &str) -> Result<(), String> {
    if !native_tag.is_empty() && native_tag == socket_tag {
        Ok(())
    } else {
        Err(
            "refusing fixture post: native run tag != live server run tag (window incarnation drift)"
                .to_owned(),
        )
    }
}

/// Live server snapshot for the selected caller: the responding PID plus
/// the caller id and run tag parsed from its server-controlled title, plus
/// the opaque server `native_handle` (a winit debug-handle description, NOT
/// a CGWID: never parsed, never compared across domains, only retained and
/// demanded stable across calls so a re-created caller window refuses as
/// incarnation drift even when the run-wide tag is unchanged).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SocketSnapshot {
    pub pid: u32,
    pub caller: String,
    pub run_tag: String,
    pub native_handle: String,
}

/// Pure `get`-reply parser over the exact fixture wire shapes (see
/// `tiler-live-fixture` `protocol::Reply` / `WindowResult` / `Snapshot`):
/// the reply must echo our request id with `ok:true`; the selected caller
/// entry must be `ok:true` with a snapshot carrying a numeric `pid`, a
/// string `title` (from which caller and run tag parse), and a nonempty
/// opaque `native_handle` string. Request-level
/// failures (including a stale-nonce `bad nonce`) map to fixed messages -
/// server error text is never passed through (it can quote our request,
/// which carries the secret). A per-window refusal without a request-level
/// error means the selected window is unknown there. Pure; unit-tested
/// against realistic wire vectors.
pub(crate) fn parse_socket_get_reply(
    reply: &serde_json::Value,
    request_id: &str,
    caller: &str,
) -> Result<SocketSnapshot, String> {
    let echo = reply
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if echo != request_id {
        return Err("refusing fixture post: socket reply id mismatch".to_owned());
    }
    if reply.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        // A per-window refusal for the selected caller (the real unknown-id
        // shape: request-level `ok:false` with no request error) diagnoses
        // precisely before the generic request mapping below.
        let unknown_window = reply
            .get("result")
            .and_then(|result| result.get("windows"))
            .and_then(serde_json::Value::as_array)
            .map(|windows| {
                windows.iter().any(|entry| {
                    entry.get("id").and_then(serde_json::Value::as_str) == Some(caller)
                        && entry.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
                })
            })
            .unwrap_or(false);
        if unknown_window {
            return Err(
                "refusing fixture post: live server reports the caller window unknown".to_owned(),
            );
        }
        let error = reply
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if error.contains("nonce") {
            return Err(
                "refusing fixture post: live server refused the nonce (stale or foreign run)"
                    .to_owned(),
            );
        }
        return Err("refusing fixture post: live server refused the request".to_owned());
    }
    let windows = reply
        .get("result")
        .and_then(|result| result.get("windows"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "refusing fixture post: socket reply lacks window results".to_owned())?;
    let entry = windows
        .iter()
        .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(caller))
        .ok_or_else(|| {
            "refusing fixture post: live server holds no such caller window".to_owned()
        })?;
    if entry.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(
            "refusing fixture post: live server reports the caller window unknown".to_owned(),
        );
    }
    let snapshot = entry
        .get("snapshot")
        .ok_or_else(|| "refusing fixture post: socket reply lacks the snapshot".to_owned())?;
    let pid = snapshot
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| "refusing fixture post: socket snapshot pid unreadable".to_owned())?;
    let title = snapshot
        .get("title")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "refusing fixture post: socket snapshot title unreadable".to_owned())?;
    let (live_caller, run_tag) = parse_fixture_title(title)?;
    if live_caller != caller {
        return Err(
            "refusing fixture post: socket snapshot caller mismatch (stale or foreign window)"
                .to_owned(),
        );
    }
    // The opaque handle binds the window incarnation (see `SocketSnapshot`):
    // absent or empty means the server no longer describes a live window.
    let native_handle = snapshot
        .get("native_handle")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if native_handle.is_empty() {
        return Err("refusing fixture post: socket snapshot native handle unreadable".to_owned());
    }
    Ok(SocketSnapshot {
        pid,
        caller: live_caller,
        run_tag,
        native_handle: native_handle.to_owned(),
    })
}

/// Pure designated-requirement extractor over `codesign -d -r -`
/// output (stdout and stderr combined): the single requirement line starting
/// at the first `designated => ` marker, up to its line end. The combined
/// output carries the `Executable=<path>` line as well (before or after the
/// requirement, depending on stream order); that line is never part of the
/// requirement. The caller compares the line against the expected fixture
/// requirement exactly. Pure; unit-tested.
pub(crate) fn extract_designated_tail(output: &str) -> Result<String, String> {
    output
        .find("designated => ")
        .map(|at| output[at..].lines().next().unwrap_or("").trim().to_owned())
        .filter(|tail| !tail.is_empty())
        .ok_or_else(|| {
            "refusing fixture post: no designated requirement in codesign output".to_owned()
        })
}

/// Pure lifetime verdict over plain data (no OS touch): the live executable
/// and launch date must equal the bound session values exactly. Drift means
/// PID reuse or relaunch and refuses. Values never travel into errors or
/// reports (no paths). Unit-tested.
pub(crate) fn lifetime_verdict(
    live_exe: Option<&str>,
    bound_exe: &str,
    live_launch: Option<&str>,
    bound_launch: &str,
) -> Result<(), String> {
    match live_exe {
        Some(exe) if exe == bound_exe => {}
        Some(_) => {
            return Err("refusing fixture post: executable drift (possible PID reuse)".to_owned());
        }
        None => {
            return Err("refusing fixture post: executable path unreadable".to_owned());
        }
    }
    match live_launch {
        Some(launch) if launch == bound_launch => {}
        Some(_) => {
            return Err(
                "refusing fixture post: launch date drift (relaunch or PID reuse)".to_owned(),
            );
        }
        None => {
            return Err(
                "refusing fixture post: launch date unreadable (no LaunchServices lifetime)"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

/// Pure Secure Input verdict over plain data (no OS touch): only observed
/// inactive proceeds. Unknown refuses here (stricter than the legacy
/// helper/TextEdit posting gates, which inherit a permissive unknown): a
/// harness that cannot tell whether Secure Input is active must not post.
/// Unit-tested.
pub(crate) fn secure_input_verdict(state: Option<bool>) -> Result<(), String> {
    match state {
        Some(false) => Ok(()),
        Some(true) => Err("secure-input: Secure Input active; refusing post".to_owned()),
        None => Err("secure-input: Secure Input state unknown; refusing post".to_owned()),
    }
}

/// Shared live session gates before bind, before each full proof, and
/// inside every commit check (no posts): SLPS symbols, AX trust, Secure
/// Input (strict: unknown refuses), session lock, post access. Same legs
/// as the helper and TextEdit posting routes except the strict unknown.
/// Any failure refuses with its stage name.
fn session_gates() -> Result<(), String> {
    if !crate::ffi::slps_available() {
        return Err("slps-unavailable: SLPS focus symbols unavailable; stop".to_owned());
    }
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err("ax-trust: AX trust not observed true; stop".to_owned());
    }
    secure_input_verdict(crate::ffi::secure_input_known())?;
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err("session: session locked; refusing post".to_owned());
        }
        crate::ffi::SessionLock::Unknown => {
            return Err("session: session unknown; refusing post".to_owned());
        }
    }
    match crate::ffi::cg_post_event_access_known() {
        Some(true) => Ok(()),
        access => Err(format!(
            "post-access: window server post access not granted ({access:?}); refusing post"
        )),
    }
}

/// One nonce-gated read-only `get` exchange with the live fixture server:
/// connect, send exactly one request line, and return the first reply line
/// echoing our request id. Absolute deadline bounds the whole exchange; no
/// polling, no subscription, no mutation. Only the matched reply is
/// returned (parsed by [`parse_socket_get_reply`]); anything else fails
/// closed. Unix-only: the fixture transport is a Unix socket on macOS.
#[cfg(unix)]
fn socket_get_reply(
    socket_path: &str,
    nonce: &str,
    caller: &str,
) -> Result<serde_json::Value, String> {
    let request = serde_json::json!({
        "v": 1,
        "request_id": SOCKET_REQUEST_ID,
        "nonce": nonce,
        "op": "get",
        "ids": [caller],
    });
    socket_round_trip(socket_path, &request, SOCKET_REQUEST_ID)
}

/// One nonce-gated request line over a fresh control connection, returning
/// the first reply line echoing `request_id`. Absolute deadline bounds the
/// whole exchange. The caller judges the reply; anything unmatched fails
/// closed. No secrets travel into errors.
#[cfg(unix)]
fn socket_round_trip(
    socket_path: &str,
    request: &serde_json::Value,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Instant;
    socket_path_valid(socket_path).map_err(|e| format!("socket: {e}"))?;
    let deadline = Instant::now() + SOCKET_DEADLINE;
    // Connect off the calling thread under the operation timeout: a wedged
    // listener backlog must not block the harness past the deadline.
    let path = socket_path.to_owned();
    let (done_tx, done_rx) = mpsc::channel::<std::io::Result<UnixStream>>();
    std::thread::Builder::new()
        .name("fixture-socket-connect".to_owned())
        .spawn(move || {
            let _ = done_tx.send(UnixStream::connect(&path));
        })
        .map_err(|e| format!("socket: connect worker spawn refused ({e})"))?;
    let mut stream = match done_rx.recv_timeout(SOCKET_OP_TIMEOUT) {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            return Err(format!("socket: connect refused ({e})"));
        }
        Err(_) => {
            return Err("socket: connect timed out".to_owned());
        }
    };
    stream
        .set_read_timeout(Some(SOCKET_OP_TIMEOUT))
        .map_err(|e| format!("socket: read timeout arm refused ({e})"))?;
    stream
        .set_write_timeout(Some(SOCKET_OP_TIMEOUT))
        .map_err(|e| format!("socket: write timeout arm refused ({e})"))?;
    let mut line = serde_json::to_string(request)
        .map_err(|e| format!("socket: request encode refused ({e})"))?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("socket: request write refused ({e})"))?;
    stream
        .flush()
        .map_err(|e| format!("socket: request flush refused ({e})"))?;
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| format!("socket: read arm refused ({e})"))?,
    );
    loop {
        if Instant::now() >= deadline {
            return Err("socket: reply deadline exceeded".to_owned());
        }
        let mut raw = String::new();
        match reader.read_line(&mut raw) {
            Ok(0) => return Err("socket: server closed before the reply".to_owned()),
            Ok(_) => {}
            Err(e) => {
                // A read timeout before the absolute deadline is retried by
                // the loop; anything past it refuses above.
                if Instant::now() >= deadline {
                    return Err("socket: reply deadline exceeded".to_owned());
                }
                return Err(format!("socket: reply read refused ({e})"));
            }
        }
        if raw.len() > 1_048_576 + 1 {
            return Err("socket: oversize frame refused".to_owned());
        }
        let value: serde_json::Value = serde_json::from_str(raw.trim_end())
            .map_err(|_| "socket: reply is not JSON".to_owned())?;
        if value.get("request_id").and_then(serde_json::Value::as_str) == Some(request_id) {
            return Ok(value);
        }
        // Our connection never subscribes, so only our reply can arrive;
        // anything else is bounded by the deadline above, never waited on.
    }
}

/// Non-Unix fallback: the fixture transport does not exist here.
#[cfg(not(unix))]
fn socket_get_reply(
    _socket_path: &str,
    _nonce: &str,
    _caller: &str,
) -> Result<serde_json::Value, String> {
    Err("socket: fixture transport is Unix-only".to_owned())
}

/// Live AX join for one fixture window: the AX window-id set for `pid` must
/// contain `wid`. A multi-window fixture owns several windows, so unlike the
/// single-helper join this is a contains check, never an exactly-one gate.
/// Every `AxElement` is constructed, used, and dropped on a dedicated
/// worker; the caller waits bounded (same 15 s bound as the helper join).
/// Refusals name the leg; nothing is guessed.
fn ax_contains_wid(pid: u32, wid: u32) -> Result<(), String> {
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("identity: pid {pid} out of range"))?;
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<bool, String>>();
    std::thread::Builder::new()
        .name(format!("fixture-ax-join-{pid}"))
        .spawn(move || {
            // Every AX handle lives and dies on this worker.
            let found: Result<bool, String> = (|| {
                let app = crate::ffi::AxElement::application(pid_i32)
                    .ok_or_else(|| format!("no AX element for pid {pid}"))?;
                let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
                let windows = app.windows().map_err(crate::ffi::ax_error_name)?;
                let mut found = false;
                for window in &windows {
                    let id = crate::ffi::ax_window_id_detailed(window)
                        .map_err(|e| format!("fixture AX window join: {e}"))?;
                    if id == wid {
                        found = true;
                    }
                }
                Ok(found)
            })();
            let _ = done_tx.send(found);
        })
        .map_err(|e| format!("identity: AX join worker spawn refused ({e})"))?;
    match done_rx.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) => Err(format!(
            "identity: record wid {wid} not among live AX windows; refusing stale target"
        )),
        Ok(Err(e)) => Err(format!("identity: {e}")),
        Err(_) => Err("identity: AX join timed out (bounded 15s)".to_owned()),
    }
}

/// Run one owned child process bounded by `timeout`, collecting piped
/// output. The child is spawned directly (no intermediate worker thread
/// that could abandon it): as long as it is still running past the
/// deadline it is killed and then reaped, so no orphan and no zombie
/// survives the return. The deadline is genuine, not pacing: a wedged
/// signer must not hang the harness, and the timeout reason is part of
/// the refusal. Piped stdout/stderr drain on dedicated reader threads so
/// a verbose child can never deadlock the exit poll on a full pipe; the
/// readers observe EOF once the child exits or is killed, and are joined
/// before return on every path.
fn run_child_bounded(
    program: &str,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<std::process::Output, String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Instant;
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{label}: spawn refused ({e})"))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let mut stdout_worker = Some(
        std::thread::Builder::new()
            .name(format!("{label}-stdout"))
            .spawn(move || {
                let mut buf = Vec::new();
                if let Some(handle) = stdout.as_mut() {
                    let _ = handle.read_to_end(&mut buf);
                }
                buf
            })
            .map_err(|e| {
                let _ = child.kill();
                let _ = child.wait();
                format!("{label}: output worker spawn refused ({e})")
            })?,
    );
    let stderr_worker = std::thread::Builder::new()
        .name(format!("{label}-stderr"))
        .spawn(move || {
            let mut buf = Vec::new();
            if let Some(handle) = stderr.as_mut() {
                let _ = handle.read_to_end(&mut buf);
            }
            buf
        })
        .map_err(|e| {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_worker.take().map(|worker| worker.join());
            format!("{label}: output worker spawn refused ({e})")
        })?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child
            .try_wait()
            .map_err(|e| format!("{label}: wait refused ({e})"))
        {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_worker.take().map(|worker| worker.join());
                let _ = stderr_worker.join();
                return Err(e);
            }
        }
        if Instant::now() >= deadline {
            // Past the deadline the owned child is stopped first, then
            // reaped: `kill` without `wait` would leave a zombie the
            // harness never collects.
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_worker.take().map(|worker| worker.join());
            let _ = stderr_worker.join();
            return Err(format!(
                "{label}: timed out past {timeout:?}; child killed and reaped; refusing"
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stdout = stdout_worker
        .take()
        .map(|worker| worker.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_worker.join().unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// Run one `codesign` invocation bounded by [`CODESIGN_TIMEOUT`] through
/// the owned-child runner above (read-only validity check plus
/// requirement read, same as the shell preflight; never signs). Bind-only:
/// no edge path calls this, so no `codesign` fork ever runs while a key
/// is held. Past the deadline the child is killed and reaped before the
/// refusal returns. `codesign` itself comes from the host PATH (CLT); no
/// path is hardcoded here.
fn run_codesign_bounded(args: &[&str], exe: &str) -> Result<std::process::Output, String> {
    let owned: Vec<String> = args
        .iter()
        .map(|arg| (*arg).to_owned())
        .chain(std::iter::once(exe.to_owned()))
        .collect();
    let owned_refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    run_child_bounded("codesign", &owned_refs, CODESIGN_TIMEOUT, "signature").map_err(|e| {
        match e
            .strip_prefix("signature: spawn refused (")
            .and_then(|tail| tail.strip_suffix(')'))
        {
            Some(inner) => format!("signature: codesign unavailable ({inner}); refusing"),
            None => e,
        }
    })
}

/// Plain-data process identity reads (no decisions): live bundle plus
/// executable plus launch date from one OS lookup. The caller refuses
/// whatever its contract requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityReads {
    pub bundle: Option<String>,
    pub exe: Option<String>,
    pub launch: Option<String>,
}

/// Live process identity for one PID (no decisions): bundle plus
/// executable plus LaunchServices launch date from one OS lookup. Refuses
/// only on an out-of-range PID; unreadable fields stay `None` for the
/// caller to judge.
pub fn fixture_process_identity(pid: u32) -> Result<IdentityReads, String> {
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("identity: pid {pid} out of range"))?;
    let identity = crate::ffi::ns_app_identity_for_pid(pid_i32);
    Ok(IdentityReads {
        bundle: identity.bundle,
        exe: identity.exe,
        launch: identity.launch_date,
    })
}

/// Bound window incarnation for one `(caller, wid)`: the run tag plus the
/// opaque server `native_handle` from the socket snapshot. The first full
/// proof binds it; every later full proof and every commit demands
/// stability. A re-created caller window (fresh handle, possibly the same
/// run-wide tag) refuses as incarnation drift. Values never travel into
/// errors or reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowIncarnation {
    pub run_tag: String,
    pub native_handle: String,
}

/// Pure incarnation verdict over plain data (no OS touch): bind the
/// candidate when nothing is bound yet, otherwise demand exact stability
/// of both the run tag and the opaque handle. Drift refuses without
/// echoing the values. Unit-tested.
pub(crate) fn incarnation_verdict(
    bound: Option<&WindowIncarnation>,
    candidate: &WindowIncarnation,
) -> Result<(), String> {
    match bound {
        None => Ok(()),
        Some(bound)
            if bound.run_tag == candidate.run_tag
                && bound.native_handle == candidate.native_handle =>
        {
            Ok(())
        }
        Some(_) => Err(
            "refusing fixture post: window incarnation drift (caller re-created or handle replaced); refusing"
                .to_owned(),
        ),
    }
}

/// Plain-data native reads for one FULL proof (gathered live before any
/// key is held, judged purely by [`check_full_reads`]): the raw socket
/// reply plus the fresh OS identity, one CG entry, and the AX contains
/// bit. Deliberately no frontmost, no oracle bit, no signature tail:
/// foreground authority belongs to the commit check (which reads it
/// last, immediately before the post), and signing identity is proven at
/// bind, never re-forked per edge.
#[derive(Debug, Clone)]
pub(crate) struct FullReads {
    pub socket_reply: serde_json::Value,
    pub bundle: Option<String>,
    pub exe: Option<String>,
    pub launch: Option<String>,
    pub cg_entry: Option<(u32, Option<String>)>,
    pub ax_contains: bool,
}

/// Plain-data native reads for one cheap COMMIT check (gathered live
/// immediately before a post, judged purely by [`check_commit_reads`]):
/// the fresh OS identity continuity, one CG entry, and the frontmost PID
/// read last. Deliberately no socket exchange, no AX enumeration, and no
/// subprocess: this is the only proof that runs while a key is held, so
/// it must stay bounded and cheap.
#[derive(Debug, Clone)]
pub(crate) struct CommitReads {
    pub exe: Option<String>,
    pub launch: Option<String>,
    pub cg_entry: Option<(u32, Option<String>)>,
    pub front_pid: Option<i32>,
}

/// Injected live boundary for the harness pipeline. The production
/// implementation ([`LiveTransport`]) performs the real OS/socket reads;
/// tests inject scripted reads to prove refusal ordering, release-once
/// behavior, and exact stages without touching the desktop.
pub(crate) trait FixtureTransport {
    /// Shared session gates (no posts).
    fn session_gates(&self) -> Result<(), String>;
    /// Fresh process identity reads for `pid` (no decisions).
    fn process_identity(&self, pid: u32) -> Result<IdentityReads, String>;
    /// Codesign requirement tail for the OS-read executable (validity plus
    /// read; comparison stays in the pure check). Bind-only: no proof or
    /// commit path calls this, so no fork ever runs while a key is held.
    fn designated_tail(&self, exe: &str) -> Result<String, String>;
    /// Full proof reads in one fresh pass (socket reply plus identity, CG,
    /// and AX reads). Runs only before the down edge, never while a key
    /// is held. Deliberately no selected-oracle read: the oracle is
    /// sampled by its own step after every blocking full read completes.
    fn full_reads(&self, sess: &BoundSession, caller: &str, wid: u32) -> Result<FullReads, String>;
    /// Exact selected-window attribution for `(pid, wid)` via the selected
    /// oracle, sampled AFTER all blocking full reads and immediately
    /// before the cheap down commit. Down path only: never while a key is
    /// held, never on the focus-acquisition path.
    fn selected_agreement(&self, pid: u32, wid: u32) -> Result<(), String>;
    /// Cheap commit reads in one fresh pass (identity continuity, CG
    /// entry, frontmost read last). The only proof that runs immediately
    /// before each post, including while a key is held: no socket, no AX
    /// enumeration, no subprocess.
    fn commit_reads(&self, sess: &BoundSession, wid: u32) -> Result<CommitReads, String>;
    /// One raw session-stream post (dispatch cost only, never delivery).
    fn post_key(&self, pid: u32, key: u16, flags: u64, down: bool) -> Result<u64, String>;
    /// Latched-interrupt check at explicit sequence points.
    fn interrupted(&self) -> bool;
}

/// Production transport: every method touches the real OS/socket.
pub(crate) struct LiveTransport;

impl FixtureTransport for LiveTransport {
    fn session_gates(&self) -> Result<(), String> {
        session_gates()
    }

    fn process_identity(&self, pid: u32) -> Result<IdentityReads, String> {
        fixture_process_identity(pid)
    }

    fn designated_tail(&self, exe: &str) -> Result<String, String> {
        // Validity plus requirement read; the comparison stays pure so a
        // drifted requirement and an unreadable one refuse distinctly.
        if exe.is_empty() {
            return Err("signature: missing executable; refusing".to_owned());
        }
        let validity = run_codesign_bounded(&["--verify", "--strict"], exe)?;
        if !validity.status.success() {
            return Err(format!(
                "signature: codesign verify refused (status {}); refusing",
                validity.status.code().unwrap_or(-1)
            ));
        }
        let read = run_codesign_bounded(&["-d", "-r", "-"], exe)?;
        if !read.status.success() {
            return Err(format!(
                "signature: designated-requirement read refused (status {}); refusing",
                read.status.code().unwrap_or(-1)
            ));
        }
        let mut combined = String::from_utf8_lossy(&read.stdout).into_owned();
        combined.push_str(&String::from_utf8_lossy(&read.stderr));
        extract_designated_tail(&combined).map_err(|e| format!("signature: {e}"))
    }

    fn full_reads(&self, sess: &BoundSession, caller: &str, wid: u32) -> Result<FullReads, String> {
        // One fresh pass, cheapest transport first; any leg may fail and
        // aborts the gather (the pure check below judges semantics).
        // Ordering is socket, identity, CG, AX: nothing slow follows the
        // target reads here, and the foreground authority is NOT read in
        // this pass at all - the commit check reads it last, immediately
        // before the post.
        let socket_reply = socket_get_reply(&sess.socket_path, &sess.nonce, caller)
            .map_err(|e| format!("full-reads: {e}"))?;
        let pid_i32 = i32::try_from(sess.pid)
            .map_err(|_| format!("full-reads: pid {} out of range", sess.pid))?;
        let identity = crate::ffi::ns_app_identity_for_pid(pid_i32);
        let cg_entry = crate::ffi::cg_window_list_all()
            .map_err(|e| format!("full-reads: CG snapshot refused ({e})"))?
            .into_iter()
            .find(|window| window.wid == wid)
            .map(|window| (window.pid, window.title));
        let ax_contains = ax_contains_wid(sess.pid, wid).is_ok();
        Ok(FullReads {
            socket_reply,
            bundle: identity.bundle,
            exe: identity.exe,
            launch: identity.launch_date,
            cg_entry,
            ax_contains,
        })
    }

    fn selected_agreement(&self, pid: u32, wid: u32) -> Result<(), String> {
        // Exact attribution sampled after every blocking full read has
        // completed (a pre-inventory sample could name a window the later
        // AX enumeration outlasts). Spans a CG inventory plus the
        // selected-focus legs, so down path only - never while held.
        crate::focus::read_agreement(pid, wid)
            .map(|_| ())
            .map_err(|e| {
                format!("refusing fixture post: selected oracle disagrees on pid {pid} wid {wid} ({e}); refusing")
            })
    }

    fn commit_reads(&self, sess: &BoundSession, wid: u32) -> Result<CommitReads, String> {
        // Cheap final authority immediately before a post: identity
        // continuity, then the CG entry, then frontmost LAST (a foreign
        // foreground - including the hosting terminal - read here refuses
        // before the window server is touched). No socket exchange, no AX
        // enumeration, no subprocess: bounded native reads only. The
        // unavoidable host calls are exactly these three; no settle sleep
        // or re-authentication is added.
        let pid_i32 = i32::try_from(sess.pid)
            .map_err(|_| format!("commit: pid {} out of range", sess.pid))?;
        let identity = crate::ffi::ns_app_identity_for_pid(pid_i32);
        let cg_entry = crate::ffi::cg_window_list_all()
            .map_err(|e| format!("commit: CG snapshot refused ({e})"))?
            .into_iter()
            .find(|window| window.wid == wid)
            .map(|window| (window.pid, window.title));
        let front_pid = crate::ffi::frontmost_app().map(|(pid, _)| pid);
        Ok(CommitReads {
            exe: identity.exe,
            launch: identity.launch_date,
            cg_entry,
            front_pid,
        })
    }

    fn post_key(&self, pid: u32, key: u16, flags: u64, down: bool) -> Result<u64, String> {
        crate::synth::raw_post_key_event_to_session(pid, key, flags, down, None)
            .map(|posted| posted.post_latency_us)
    }

    fn interrupted(&self) -> bool {
        crate::space_drag_diag_live::diag_interrupted()
    }
}

/// Pure full proof over gathered reads (no OS touch): socket
/// authentication first, then bundle, lifetime, CG/title/run-tag join, and
/// AX join. No foreground read here (the commit owns it, last), no oracle
/// (the selected step owns it, after every blocking read), and no
/// signature tail (bind owns it). First failure wins, so gate ordering is
/// pinned by test. Returns the live server snapshot plus the candidate
/// window incarnation on success.
pub(crate) fn check_full_reads(
    sess: &BoundSession,
    caller: &str,
    wid: u32,
    reads: &FullReads,
) -> Result<(SocketSnapshot, WindowIncarnation), String> {
    let snapshot = parse_socket_get_reply(&reads.socket_reply, SOCKET_REQUEST_ID, caller)?;
    if snapshot.pid != sess.pid {
        return Err(format!(
            "refusing fixture post: live server pid {} != announced pid {}; refusing",
            snapshot.pid, sess.pid
        ));
    }
    bundle_verdict(reads.bundle.as_deref())?;
    lifetime_verdict(
        reads.exe.as_deref(),
        &sess.exe,
        reads.launch.as_deref(),
        &sess.launch,
    )?;
    let (owner_pid, title) = reads.cg_entry.as_ref().ok_or_else(|| {
        format!("refusing fixture post: wid {wid} not present in CGWindowList; refusing without exact PID association")
    })?;
    if *owner_pid != sess.pid {
        return Err(format!(
            "refusing fixture post: wid {wid} belongs to pid {owner_pid} (not fixture pid {}); refusing",
            sess.pid
        ));
    }
    let title = title.as_deref().ok_or_else(|| {
        "refusing fixture post: target title unreadable; refusing stale target".to_owned()
    })?;
    let (native_caller, native_tag) = parse_fixture_title(title)?;
    if native_caller != caller {
        return Err(
            "refusing fixture post: native title caller mismatch (stale or foreign window)"
                .to_owned(),
        );
    }
    run_tag_verdict(&native_tag, &snapshot.run_tag)?;
    if !reads.ax_contains {
        return Err(format!(
            "refusing fixture post: record wid {wid} not among live AX windows; refusing stale target"
        ));
    }
    let incarnation = WindowIncarnation {
        run_tag: snapshot.run_tag.clone(),
        native_handle: snapshot.native_handle.clone(),
    };
    Ok((snapshot, incarnation))
}

/// Pure cheap commit check over gathered reads (no OS touch): process
/// continuity first (no fork: executable/launch still equal the bound
/// values), then the CG owner/title/run-tag join against the bound window
/// incarnation, then - on the post path - the fresh frontmost PID read
/// LAST. First failure wins. The frontmost gate is last by construction:
/// nothing slow follows it before the post.
pub(crate) fn check_commit_reads(
    sess: &BoundSession,
    caller: &str,
    wid: u32,
    reads: &CommitReads,
    incarnation: &WindowIncarnation,
    require_foreground: bool,
) -> Result<(), String> {
    lifetime_verdict(
        reads.exe.as_deref(),
        &sess.exe,
        reads.launch.as_deref(),
        &sess.launch,
    )?;
    let (owner_pid, title) = reads.cg_entry.as_ref().ok_or_else(|| {
        format!("refusing fixture post: wid {wid} not present in CGWindowList; refusing without exact PID association")
    })?;
    if *owner_pid != sess.pid {
        return Err(format!(
            "refusing fixture post: wid {wid} belongs to pid {owner_pid} (not fixture pid {}); refusing",
            sess.pid
        ));
    }
    let title = title.as_deref().ok_or_else(|| {
        "refusing fixture post: target title unreadable; refusing stale target".to_owned()
    })?;
    let (native_caller, native_tag) = parse_fixture_title(title)?;
    if native_caller != caller {
        return Err(
            "refusing fixture post: native title caller mismatch (stale or foreign window)"
                .to_owned(),
        );
    }
    if native_tag != incarnation.run_tag {
        return Err(
            "refusing fixture post: native run tag != bound window incarnation (window incarnation drift)"
                .to_owned(),
        );
    }
    if require_foreground {
        front_verdict(reads.front_pid, sess.pid)?;
    }
    Ok(())
}

/// Bound fixture session: announced PID/socket/nonce plus the live
/// executable and launch date the bind-time signature verified. The nonce
/// never travels into errors or reports. The expected requirement itself
/// is compared at bind and not retained: no edge re-reads it, so no fork
/// ever runs past bind.
#[derive(Debug, Clone)]
pub(crate) struct BoundSession {
    pub pid: u32,
    pub socket_path: String,
    pub nonce: String,
    pub exe: String,
    pub launch: String,
}

/// Reusable session/post driver over an injected transport (production uses
/// [`LiveTransport`): [`FixtureDriver::bind`] verifies the signing identity
/// once and binds the server-level session; [`FixtureDriver::post`] and
/// [`FixtureDriver::focus`] drive one target window each with a full proof
/// outside any held key, an exact selected attribution sampled after the
/// blocking reads (post path only), plus a cheap commit immediately before
/// each input. Binding additionally requires a leg admission
/// ([`LegAdmission`]): the admission's fixture PID must equal the announced
/// PID, so a path that skips the leg's mandatory baselines cannot bind, and
/// each post checks the chord's scope against the admission (space-mutating
/// chords need the leg's executed return probe first). No launching, no
/// subprocess per action, no intentional pacing: repeat
/// calls reuse the same bound session and guard continuity (a future
/// in-process sequential/burst driver paces itself around these calls).
pub(crate) struct FixtureDriver<'a, T: FixtureTransport> {
    session: BoundSession,
    transport: &'a T,
    admission: LegAdmission,
    windows: RefCell<HashMap<(String, u32), WindowIncarnation>>,
}

impl<'a, T: FixtureTransport> std::fmt::Debug for FixtureDriver<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Session metadata only: nonce and requirement never format.
        f.debug_struct("FixtureDriver")
            .field("pid", &self.session.pid)
            .finish_non_exhaustive()
    }
}

impl<'a, T: FixtureTransport> FixtureDriver<'a, T> {
    /// Bind a server-level session: parse the announcement, demand the
    /// presented nonce claim equals it, demand the leg admission binds this
    /// exact announced PID (a stale or foreign admission refuses here, so no
    /// legacy path can skip the mandatory baselines), prove session gates,
    /// then bind live bundle/executable/launch plus the code signature
    /// against the expected requirement. The signature is verified HERE,
    /// once per driver - never per edge, never while a key is held. Refuses
    /// before any post. Usage shapes exit 2; live gates exit 1.
    pub fn bind(
        transport: &'a T,
        announcement: &str,
        nonce: &str,
        signer: &str,
        admission: LegAdmission,
    ) -> Result<Self, (i32, String)> {
        let announced = parse_fixture_announcement(announcement)
            .map_err(|e| (2, format!("announcement refused: {e}")))?;
        if nonce.is_empty() {
            return Err((2, "fixture command needs a nonempty --nonce".to_owned()));
        }
        if announced.nonce != nonce {
            return Err((
                2,
                "refusing fixture command: nonce mismatch (stale or foreign run)".to_owned(),
            ));
        }
        if !admission.is_live() {
            return Err((
                2,
                "refusing fixture command: leg admission revoked (leg finished); refusing replay"
                    .to_owned(),
            ));
        }
        if admission.fixture_pid() != announced.pid {
            return Err((
                2,
                "refusing fixture command: leg admission binds a different fixture run (stale or foreign admission); refusing without a fresh leg".to_owned(),
            ));
        }
        if signer.trim().is_empty() {
            return Err((2, "fixture command needs a nonempty --signer".to_owned()));
        }
        let pid = announced.pid;
        if pid <= 1 {
            return Err((2, format!("refusing fixture command to reserved pid {pid}")));
        }
        if pid == std::process::id() {
            return Err((
                2,
                "refusing fixture command to the controller itself".to_owned(),
            ));
        }
        if i32::try_from(pid).is_err() {
            return Err((
                2,
                format!("refusing fixture command: pid {pid} out of range"),
            ));
        }
        transport.session_gates().map_err(|e| {
            (
                1,
                serde_json::json!({"stage": "session", "error": e}).to_string(),
            )
        })?;
        let identity = transport.process_identity(pid).map_err(|e| {
            (
                1,
                serde_json::json!({"stage": "identity", "error": e}).to_string(),
            )
        })?;
        bundle_verdict(identity.bundle.as_deref()).map_err(|e| {
            (
                1,
                serde_json::json!({"stage": "identity", "error": format!("identity: {e}")})
                    .to_string(),
            )
        })?;
        let exe = identity.exe.filter(|exe| !exe.is_empty()).ok_or((
            1,
            serde_json::json!({"stage": "identity", "error": "identity: executable path unreadable; refusing"})
                .to_string(),
        ))?;
        let launch = identity.launch.filter(|launch| !launch.is_empty()).ok_or((
            1,
            serde_json::json!({"stage": "identity", "error": "identity: launch date unreadable (no LaunchServices lifetime); refusing"})
                .to_string(),
        ))?;
        let tail = transport.designated_tail(&exe).map_err(|e| {
            (
                1,
                serde_json::json!({"stage": "signature", "error": e}).to_string(),
            )
        })?;
        if tail != signer.trim() {
            return Err((
                1,
                serde_json::json!({"stage": "signature", "error": "signature: designated requirement mismatch (not the expected fixture signer); refusing"})
                    .to_string(),
            ));
        }
        Ok(Self {
            session: BoundSession {
                pid,
                socket_path: announced.socket,
                nonce: announced.nonce,
                exe,
                launch,
            },
            transport,
            admission,
            windows: RefCell::new(HashMap::new()),
        })
    }

    /// Full proof through the transport (gates plus fresh socket/identity/
    /// CG/AX reads plus the pure check), run only before the down edge
    /// while nothing is held. Binds the window incarnation on first use
    /// and demands its stability afterwards, so a re-created caller window
    /// refuses here even when the run-wide tag is unchanged. Returns the
    /// live server snapshot. Deliberately no selected-oracle sample: the
    /// caller runs that step after this returns, so attribution can never
    /// predate the blocking inventory.
    fn prove_full(&self, caller: &str, wid: u32, edge: &str) -> Result<SocketSnapshot, String> {
        self.transport
            .session_gates()
            .map_err(|e| format!("{edge}: {e}"))?;
        let reads = self
            .transport
            .full_reads(&self.session, caller, wid)
            .map_err(|e| format!("{edge}: {e}"))?;
        let (snapshot, candidate) = check_full_reads(&self.session, caller, wid, &reads)
            .map_err(|e| format!("{edge}: {e}"))?;
        let mut windows = self.windows.borrow_mut();
        let key = (caller.to_owned(), wid);
        incarnation_verdict(windows.get(&key), &candidate).map_err(|e| format!("{edge}: {e}"))?;
        windows.entry(key).or_insert(candidate);
        Ok(snapshot)
    }

    /// Exact selected-window attribution for the down path, sampled after
    /// the full proof's blocking reads complete and before the cheap down
    /// commit runs. A same-PID focus move to another fixture window after
    /// the full proof refuses here, before the window server is touched.
    fn prove_selected(&self, wid: u32, edge: &str) -> Result<(), String> {
        self.transport
            .selected_agreement(self.session.pid, wid)
            .map_err(|e| format!("{edge}: {e}"))
    }

    /// Cheap commit check through the transport (gates plus fresh
    /// identity/CG/frontmost reads plus the pure check against the bound
    /// window incarnation), run immediately before EACH post - the only
    /// proof that runs while a key is held. No socket, no AX enumeration,
    /// no subprocess. Requires a previously bound incarnation: a commit
    /// before any full proof is a driver logic error and refuses.
    fn prove_commit(
        &self,
        caller: &str,
        wid: u32,
        edge: &str,
        require_foreground: bool,
    ) -> Result<(), String> {
        self.transport
            .session_gates()
            .map_err(|e| format!("{edge}: {e}"))?;
        let key = (caller.to_owned(), wid);
        let incarnation = self.windows.borrow().get(&key).cloned().ok_or_else(|| {
            format!("{edge}: refusing fixture post with no bound window incarnation")
        })?;
        let reads = self
            .transport
            .commit_reads(&self.session, wid)
            .map_err(|e| format!("{edge}: {e}"))?;
        check_commit_reads(
            &self.session,
            caller,
            wid,
            &reads,
            &incarnation,
            require_foreground,
        )
        .map_err(|e| format!("{edge}: {e}"))
    }

    /// Harness-owned bypass release for a held key: exactly one gateless
    /// cleared-flags key-up through the injected transport, then the guard
    /// is disarmed whatever the outcome (bounded: no retries, and `Drop`
    /// stays inert so no second release can follow). Success is
    /// `Ok(latency)`; a failed post is an explicit `Err` that the caller
    /// reports as release failure - never silent success, and never
    /// counted as delivery evidence (only the gated up post reports
    /// delivery). The shared [`EdgeGuard`](crate::textedit_post::EdgeGuard)
    /// primitive itself is untouched.
    fn bypass_release(
        &self,
        guard: &mut crate::textedit_post::EdgeGuard,
        pid: u32,
        key_code: u16,
    ) -> Result<u64, String> {
        if !guard.is_pending() {
            return Err("bypass: no release owed (nothing held)".to_owned());
        }
        let outcome = self.transport.post_key(pid, key_code, 0, false);
        // Disarm exactly once on every path: one attempt, bounded, no
        // retries; a failed attempt is reported, not retried.
        guard.note_released();
        outcome
    }

    /// Post ONE product-armed chord down/up to the exact fixture window.
    /// Exit codes: 2 usage/validation, 1 gate/proof/post failure with
    /// evidence. Proof microseconds (guard cost) report separately from
    /// dispatch microseconds. Every refusal is logged to stderr as metadata
    /// (stages and IDs only). Once the down edge has posted, every return
    /// path releases through the guard exactly once (gateless
    /// cleared-modifier key-up); unwinding through the guard releases via
    /// `Drop`.
    pub fn post(
        &self,
        caller: &str,
        wid: u32,
        chord_name: &str,
        modifier: ProbeModifier,
    ) -> Result<String, (i32, String)> {
        let err_json = |stage: &str, error: String| -> (i32, String) {
            (
                1,
                serde_json::json!({"stage": stage, "error": error}).to_string(),
            )
        };
        let refuse = |stage: &str, error: String| -> (i32, String) {
            eprintln!("fixture-post refused at {stage}: {error}");
            err_json(stage, error)
        };
        if !self.admission.is_live() {
            return Err(refuse(
                "proof-down",
                "refusing fixture post: leg admission revoked (leg finished); refusing replay"
                    .to_owned(),
            ));
        }
        if !caller_id_valid(caller) {
            return Err((2, "refusing fixture post: caller id malformed".to_owned()));
        }
        if wid == 0 {
            return Err((2, "refusing fixture post to window id 0".to_owned()));
        }
        // Fixed vocabulary only: the directional/numbered leg first, then
        // the bound relative/previous leg. No generic raw-key API. The
        // chord's admission scope gates here: space-mutating chords need a
        // leg admission that proved the return route first.
        let want_scope = crate::live_leg::required_scope(chord_name)
            .ok_or_else(|| (2, format!("unknown --chord '{chord_name}' (see help)")))?;
        if !self.admission.scope().covers(want_scope) {
            return Err((
                2,
                "refusing fixture post: post-only leg admission cannot run a Space-mutating chord (prove the return route first)".to_owned(),
            ));
        }
        let (key_code_u32, want_flags, action_label): (u32, u64, &'static str) =
            match resolve_fixture_chord(chord_name) {
                Some(chord) => (chord.key_code, fixture_flags(modifier, chord), chord.action),
                None => match resolve_fixture_bound(chord_name) {
                    Some(binding) => (
                        binding.key_code,
                        crate::workspace_input::effective_flags_for(modifier, &binding),
                        "select",
                    ),
                    None => {
                        return Err((2, format!("unknown --chord '{chord_name}' (see help)")));
                    }
                },
            };
        let pid = self.session.pid;
        let key_code = key_code_u32 as u16;
        // Handled-termination arm: SIGINT/SIGTERM latch the shared
        // diagnostic flag from here until return (unregistered on drop,
        // including unwind). Registration failure refuses BEFORE any post.
        // The latch is consulted at explicit sequence points below - never
        // a poll loop - and a latched interrupt releases first like any
        // up-leg failure.
        let _interrupt = crate::space_drag_diag_live::InterruptArmed::arm()
            .map_err(|e| refuse("interrupt", e))?;
        let mut guard = crate::textedit_post::EdgeGuard::new(pid, key_code);
        // Down edge: full proof while nothing is held, then the exact
        // selected attribution sampled after every blocking read, then the
        // cheap commit immediately before the post. A latched interrupt
        // before the post refuses with nothing held. `down_proof_us`
        // covers full plus selected plus commit (guard cost, never
        // conflated with dispatch cost); `up_proof_us` covers the held-key
        // commit alone.
        let down_start = std::time::Instant::now();
        if let Err(error) = self.prove_full(caller, wid, "edge-down") {
            return Err((
                1,
                serde_json::json!({"stage": "proof-down", "error": error, "released_bypass": false})
                    .to_string(),
            ));
        }
        if let Err(error) = self.prove_selected(wid, "edge-down") {
            return Err((
                1,
                serde_json::json!({"stage": "proof-down", "error": error, "released_bypass": false})
                    .to_string(),
            ));
        }
        if let Err(error) = self.prove_commit(caller, wid, "commit-down", true) {
            return Err((
                1,
                serde_json::json!({"stage": "proof-down", "error": error, "released_bypass": false})
                    .to_string(),
            ));
        }
        let down_proof_us = down_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        if self.transport.interrupted() {
            return Err((
                1,
                serde_json::json!({"stage": "interrupted", "error": "interrupted: SIGINT/SIGTERM latched; refusing post", "released_bypass": false})
                    .to_string(),
            ));
        }
        let down_post_us = self
            .transport
            .post_key(pid, key_code, want_flags, true)
            .map_err(|e| refuse("post-down", format!("post-down: {e}")))?;
        guard.note_down_posted();
        // Every path below releases exactly once through the harness-owned
        // bypass (one gateless cleared-flags key-up via the injected
        // transport, disarmed whatever the outcome); unwind releases via
        // `Drop`. A bypass result is explicit: success carries
        // `released_bypass:true`, a failed attempt carries the release
        // cause and is never delivery evidence.
        let held_release = |guard: &mut crate::textedit_post::EdgeGuard| -> (bool, Option<String>) {
            match self.bypass_release(guard, pid, key_code) {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e)),
            }
        };
        // A latched interrupt with a key held releases immediately instead
        // of committing the up edge: no slow work runs while held.
        if self.transport.interrupted() {
            let (released_bypass, release_error) = held_release(&mut guard);
            eprintln!(
                "fixture-post interrupted after posted down; bypass release posted={released_bypass} release_error={release_error:?} (pid={pid} wid={wid} caller={caller})"
            );
            return Err((
                1,
                serde_json::json!({"stage": "interrupted", "error": "interrupted: SIGINT/SIGTERM latched after posted down", "released_bypass": released_bypass, "release_error": release_error})
                    .to_string(),
            ));
        }
        // Up edge: ONLY the cheap commit runs while the key is held (no
        // socket, no AX enumeration, no subprocess); then the gated up
        // disarms the guard. A lost commit releases unconditionally.
        let up_start = std::time::Instant::now();
        if let Err(error) = self.prove_commit(caller, wid, "commit-up", true) {
            let (released_bypass, release_error) = held_release(&mut guard);
            eprintln!(
                "fixture-post up commit failed after posted down; bypass release posted={released_bypass} release_error={release_error:?} (pid={pid} wid={wid} caller={caller})"
            );
            return Err((
                1,
                serde_json::json!({"stage": "proof-up", "error": error, "released_bypass": released_bypass, "release_error": release_error})
                    .to_string(),
            ));
        }
        let up_proof_us = up_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        if self.transport.interrupted() {
            let (released_bypass, release_error) = held_release(&mut guard);
            eprintln!(
                "fixture-post interrupted after posted down; bypass release posted={released_bypass} release_error={release_error:?} (pid={pid} wid={wid} caller={caller})"
            );
            return Err((
                1,
                serde_json::json!({"stage": "interrupted", "error": "interrupted: SIGINT/SIGTERM latched after posted down", "released_bypass": released_bypass, "release_error": release_error})
                    .to_string(),
            ));
        }
        match self.transport.post_key(pid, key_code, want_flags, false) {
            Ok(up_post_us) => {
                guard.note_released();
                let report = FixturePostReport {
                    outcome: "posted".to_owned(),
                    pid,
                    wid,
                    caller: caller.to_owned(),
                    chord: chord_name.to_owned(),
                    action: action_label.to_owned(),
                    key_code: key_code_u32,
                    flags: want_flags,
                    down_proof_us,
                    down_post_us,
                    up_proof_us,
                    up_post_us,
                    released_bypass: false,
                    synthetic: true,
                    note: "development-only synthetic CG session-stream post of a product-armed chord to the exact signed-fixture window (never physical hardware, never helper input); delivery is proven harness-side by the tile effect plus the tap counters, never this dispatch receipt"
                        .to_owned(),
                };
                serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
            }
            Err(e) => {
                let (released_bypass, release_error) = held_release(&mut guard);
                eprintln!(
                    "fixture-post up post failed after posted down; bypass release posted={released_bypass} release_error={release_error:?} (pid={pid} wid={wid} caller={caller})"
                );
                Err((
                    1,
                    serde_json::json!({"stage": "post-up", "error": format!("post-up: {e}"), "released_bypass": released_bypass, "release_error": release_error})
                        .to_string(),
                ))
            }
        }
    }

    /// Focus one exact fixture window via the SLPS route with agreement
    /// proof (foreground acquisition: the commit omits the frontmost gate
    /// so it can acquire from another window; everything else is proven
    /// before dispatch). No synthetic input. Exit codes: 2
    /// usage/validation, 1 gate/dispatch/agreement failure.
    pub fn focus(&self, caller: &str, wid: u32) -> Result<String, (i32, String)> {
        let err_json = |stage: &str, error: String| -> (i32, String) {
            (
                1,
                serde_json::json!({"stage": stage, "error": error}).to_string(),
            )
        };
        let refuse = |stage: &str, error: String| -> (i32, String) {
            eprintln!("fixture-focus refused at {stage}: {error}");
            err_json(stage, error)
        };
        if !self.admission.is_live() {
            return Err(refuse(
                "focus-proof",
                "refusing fixture focus: leg admission revoked (leg finished); refusing replay"
                    .to_owned(),
            ));
        }
        if !caller_id_valid(caller) {
            return Err((2, "refusing fixture focus: caller id malformed".to_owned()));
        }
        if wid == 0 {
            return Err((2, "refusing fixture focus to window id 0".to_owned()));
        }
        let proof_start = std::time::Instant::now();
        if let Err(error) = self.prove_full(caller, wid, "edge-focus") {
            return Err(refuse("focus-proof", error));
        }
        if let Err(error) = self.prove_commit(caller, wid, "commit-focus", false) {
            return Err(refuse("focus-proof", error));
        }
        let proof_us = proof_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        let dispatch = crate::focus::dispatch_focus(self.session.pid, wid)
            .map_err(|e| refuse("focus-dispatch", e.to_string()))?;
        let (agreement, latency_ms) =
            crate::focus::await_agreement(self.session.pid, wid, Duration::from_secs(5))
                .map_err(|e| refuse("focus-agreement", e.to_string()))?;
        let report = FixtureFocusReport {
            outcome: "completed".to_owned(),
            pid: self.session.pid,
            wid,
            caller: caller.to_owned(),
            proof_us,
            dispatch,
            agreement,
            focus_latency_ms: latency_ms,
            synthetic: false,
            note: "SLPS dispatch to the exact signed-fixture window with selected-oracle agreement; foreground acquisition only, no synthetic input"
                .to_owned(),
        };
        serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
    }
}

#[derive(Debug, Clone, Serialize)]
struct FixturePostReport {
    outcome: String,
    pid: u32,
    wid: u32,
    caller: String,
    chord: String,
    action: String,
    key_code: u32,
    flags: u64,
    down_proof_us: u64,
    down_post_us: u64,
    up_proof_us: u64,
    up_post_us: u64,
    released_bypass: bool,
    synthetic: bool,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
struct FixtureFocusReport {
    outcome: String,
    pid: u32,
    wid: u32,
    caller: String,
    proof_us: u64,
    dispatch: crate::focus::FocusDispatch,
    agreement: crate::focus::FocusAgreement,
    focus_latency_ms: u64,
    synthetic: bool,
    note: String,
}

/// CLI entry parameters for one fixture post: the exact fixture window,
/// one product-armed chord, and the leg admission that proves the mandatory
/// baselines. Bundled so the entry keeps a narrow call shape.
pub struct FixturePostArgs<'a> {
    pub announcement: &'a str,
    pub nonce: &'a str,
    pub signer: &'a str,
    pub caller: &'a str,
    pub wid: u32,
    pub chord_name: &'a str,
    pub modifier: ProbeModifier,
    pub admission: LegAdmission,
}

/// CLI entry: bind a live session (through the leg admission that proves
/// the mandatory baselines), then post ONE product-armed chord down/up to
/// the exact fixture window. See [`FixtureDriver`] for the contract. Exit
/// codes: 2 usage/validation, 1 gate/proof/post failure.
pub fn run_fixture_post(args: FixturePostArgs<'_>) -> Result<String, (i32, String)> {
    if args.nonce.is_empty() {
        return Err((2, "fixture-post needs a nonempty --nonce".to_owned()));
    }
    let live = LiveTransport;
    let driver = FixtureDriver::bind(
        &live,
        args.announcement,
        args.nonce,
        args.signer,
        args.admission,
    )?;
    driver.post(args.caller, args.wid, args.chord_name, args.modifier)
}

/// CLI entry: bind a live session (through the leg admission that proves
/// the mandatory baselines), then focus one exact fixture window via the
/// SLPS route with agreement proof. See [`FixtureDriver`] for the contract.
/// Exit codes: 2 usage/validation, 1 gate/dispatch/agreement failure.
pub fn run_fixture_focus(
    announcement: &str,
    nonce: &str,
    signer: &str,
    caller: &str,
    wid: u32,
    admission: LegAdmission,
) -> Result<String, (i32, String)> {
    if nonce.is_empty() {
        return Err((2, "fixture-focus needs a nonempty --nonce".to_owned()));
    }
    let live = LiveTransport;
    let driver = FixtureDriver::bind(&live, announcement, nonce, signer, admission)?;
    driver.focus(caller, wid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProbeModifier;
    use std::cell::RefCell;

    fn announcement(nonce: &str) -> String {
        serde_json::json!({
            "socket": "/tmp/owned-fixture/fixture.sock",
            "pid": 4242,
            "nonce": nonce,
        })
        .to_string()
    }

    const NONCE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const SIGNER: &str = "designated => identifier \"io.github.omnitiler.live-fixture\" and anchor apple generic and certificate leaf[subject.CN] = \"Fixture Test Signer\"";

    /// Space-mutating test admission for the announced fixture PID: every
    /// scripted post below runs through the same admission gate as
    /// production (PID-bound, scope-checked).
    fn test_admission() -> LegAdmission {
        LegAdmission::admission_for_test(4242, crate::live_leg::AdmissionScope::SpaceMutating)
    }

    fn socket_reply(pid: u32, title: &str) -> serde_json::Value {
        socket_reply_handle(pid, title, "handle-live-1")
    }

    fn socket_reply_handle(pid: u32, title: &str, handle: &str) -> serde_json::Value {
        serde_json::json!({
            "v": 1,
            "request_id": SOCKET_REQUEST_ID,
            "ok": true,
            "result": {"windows": [
                {"id": "a", "ok": true,
                 "snapshot": {"id": "a", "pid": pid, "backend": "macos",
                              "native_handle": handle, "title": title,
                              "requested": {}, "configured": {},
                              "observed": {}, "unsupported": [], "settled": true},
                 "fields": []}
            ]},
        })
    }

    fn good_full() -> FullReads {
        FullReads {
            socket_reply: socket_reply(4242, "OmniTilerTestWindow:a:run-live"),
            bundle: Some(FIXTURE_BUNDLE.to_owned()),
            exe: Some("/live-exe".to_owned()),
            launch: Some("launch-live".to_owned()),
            cg_entry: Some((4242, Some("OmniTilerTestWindow:a:run-live".to_owned()))),
            ax_contains: true,
        }
    }

    fn good_commit() -> CommitReads {
        CommitReads {
            exe: Some("/live-exe".to_owned()),
            launch: Some("launch-live".to_owned()),
            cg_entry: Some((4242, Some("OmniTilerTestWindow:a:run-live".to_owned()))),
            front_pid: Some(4242),
        }
    }

    fn good_session() -> BoundSession {
        BoundSession {
            pid: 4242,
            socket_path: "/tmp/owned-fixture/fixture.sock".to_owned(),
            nonce: NONCE.to_owned(),
            exe: "/live-exe".to_owned(),
            launch: "launch-live".to_owned(),
        }
    }

    fn bound_incarnation() -> WindowIncarnation {
        WindowIncarnation {
            run_tag: "run-live".to_owned(),
            native_handle: "handle-live-1".to_owned(),
        }
    }

    /// Scripted transport: every live leg returns canned reads; posts are
    /// counted with exact arguments; interruption latches on demand.
    /// `full_rest` replaces `full_first` from the second `full_reads`
    /// call on (and likewise `commit_rest` for `commit_reads`), so
    /// mid-flight drift (clean down edge, lost up commit) is scriptable.
    /// Call counters prove the held-key path runs no full proof and no
    /// signature fork: `designated_calls` must stay at the bind count
    /// across repeat posts. `order` records the live-leg sequence so the
    /// full -> selected -> commit -> down ordering is pinned exactly.
    struct ScriptTransport {
        gates_error: Option<String>,
        identity: Result<IdentityReads, String>,
        designated: Result<String, String>,
        designated_calls: RefCell<u32>,
        full_first: Result<FullReads, String>,
        full_rest: Option<Result<FullReads, String>>,
        full_calls: RefCell<u32>,
        selected: Result<(), String>,
        selected_calls: RefCell<u32>,
        commit_first: Result<CommitReads, String>,
        commit_rest: Option<Result<CommitReads, String>>,
        commit_calls: RefCell<u32>,
        order: RefCell<Vec<&'static str>>,
        posts: RefCell<Vec<(u32, u16, u64, bool)>>,
        fail_up_post: bool,
        fail_bypass_post: bool,
        interrupt: RefCell<bool>,
        latch_interrupt_after_down: bool,
    }

    impl ScriptTransport {
        fn passing() -> Self {
            Self {
                gates_error: None,
                identity: Ok(IdentityReads {
                    bundle: Some(FIXTURE_BUNDLE.to_owned()),
                    exe: Some("/live-exe".to_owned()),
                    launch: Some("launch-live".to_owned()),
                }),
                designated: Ok(SIGNER.to_owned()),
                designated_calls: RefCell::new(0),
                full_first: Ok(good_full()),
                full_rest: None,
                full_calls: RefCell::new(0),
                selected: Ok(()),
                selected_calls: RefCell::new(0),
                commit_first: Ok(good_commit()),
                commit_rest: None,
                commit_calls: RefCell::new(0),
                order: RefCell::new(Vec::new()),
                posts: RefCell::new(Vec::new()),
                fail_up_post: false,
                fail_bypass_post: false,
                interrupt: RefCell::new(false),
                latch_interrupt_after_down: false,
            }
        }
    }

    impl FixtureTransport for ScriptTransport {
        fn session_gates(&self) -> Result<(), String> {
            self.gates_error.clone().map_or(Ok(()), Err)
        }
        fn process_identity(&self, _pid: u32) -> Result<IdentityReads, String> {
            self.identity.clone()
        }
        fn designated_tail(&self, _exe: &str) -> Result<String, String> {
            *self.designated_calls.borrow_mut() += 1;
            self.designated.clone()
        }
        fn full_reads(
            &self,
            _sess: &BoundSession,
            _caller: &str,
            _wid: u32,
        ) -> Result<FullReads, String> {
            let mut calls = self.full_calls.borrow_mut();
            *calls += 1;
            self.order.borrow_mut().push("full");
            if *calls >= 2
                && let Some(rest) = self.full_rest.clone()
            {
                return rest;
            }
            self.full_first.clone()
        }
        fn selected_agreement(&self, _pid: u32, _wid: u32) -> Result<(), String> {
            *self.selected_calls.borrow_mut() += 1;
            self.order.borrow_mut().push("selected");
            self.selected.clone()
        }
        fn commit_reads(&self, _sess: &BoundSession, _wid: u32) -> Result<CommitReads, String> {
            let mut calls = self.commit_calls.borrow_mut();
            *calls += 1;
            self.order.borrow_mut().push("commit");
            if *calls >= 2
                && let Some(rest) = self.commit_rest.clone()
            {
                return rest;
            }
            self.commit_first.clone()
        }
        fn post_key(&self, pid: u32, key: u16, flags: u64, down: bool) -> Result<u64, String> {
            self.posts.borrow_mut().push((pid, key, flags, down));
            self.order
                .borrow_mut()
                .push(if down { "post-down" } else { "post-up" });
            if down && self.latch_interrupt_after_down {
                // Mid-flight latch: the next sequence-point check observes
                // a held key, like a real SIGINT arriving after the down.
                *self.interrupt.borrow_mut() = true;
            }
            // The gated up post (arm-carrying chord flags) fails when
            // injected; the cleared-flags bypass release fails only when
            // its own injection is armed, so bypass-failure reporting is
            // observable exactly.
            if !down && flags != 0 && self.fail_up_post {
                return Err("injected up-post failure".to_owned());
            }
            if !down && flags == 0 && self.fail_bypass_post {
                return Err("injected bypass-post failure".to_owned());
            }
            Ok(11)
        }
        fn interrupted(&self) -> bool {
            *self.interrupt.borrow()
        }
    }

    #[test]
    fn vocabulary_admits_only_product_armed_fixture_chords() {
        // Directional focus/move letters plus left/right arrows resolve with
        // the owned-post key/arms (single home, no new hotkeys).
        for name in [
            "focus-h",
            "focus-j",
            "focus-k",
            "focus-l",
            "focus-left",
            "focus-right",
            "move-h",
            "move-j",
            "move-k",
            "move-l",
            "move-left",
            "move-right",
        ] {
            let owned = crate::owned_control::resolve_chord(name).unwrap();
            let chord = resolve_fixture_chord(name).unwrap();
            assert_eq!(chord.key_code, owned.key_code, "{name}");
            assert_eq!(chord.shift, owned.shift, "{name}");
            assert_eq!(chord.alt, owned.alt, "{name}");
            assert!(!chord.aux, "{name}");
            assert_eq!(chord.action, owned.action, "{name}");
        }
        // Numbered selects/send-follows 0..9 resolve with the product arms.
        for index in 0..=9u32 {
            for (prefix, shift, action) in [("select", false, "select"), ("send", true, "send")] {
                let name = format!("{prefix}-{index}");
                let chord = resolve_fixture_chord(&name).unwrap();
                let desktop = if index == 0 { 10 } else { index };
                assert_eq!(
                    chord.key_code,
                    crate::desktop_shortcuts::desktop_digit_keycode(desktop).unwrap(),
                    "{name}"
                );
                assert_eq!(chord.shift, shift, "{name}");
                assert!(!chord.alt, "{name}");
                assert!(!chord.aux, "{name}");
                assert_eq!(chord.action, action, "{name}");
            }
        }
        // Helper-only chords, TextEdit-only transports, stays, and arbitrary
        // keys refuse: no substitute chord is ever ridden.
        for name in [
            "",
            "auto",
            "focus-up",
            "focus-down",
            "move-up",
            "float-g",
            "grow-h",
            "grow-left",
            "shrink-h",
            "shrink-right",
            "orientation-o",
            "escape",
            "stay-1",
            "stay-2",
            "stay-0",
            "select-10",
            "send-10",
            "select-",
            "h",
            "ctrl-h",
            "a",
            "space",
            "rel-prev",
            "rel-next",
            "prev-toggle",
            "workspace-select-1",
        ] {
            assert!(resolve_fixture_chord(name).is_none(), "{name}");
        }
        // The bound relative/previous names admit only through the catalogue
        // leg (same admission as the TextEdit bound leg).
        for name in ["rel-prev", "rel-next", "prev-toggle"] {
            assert!(resolve_fixture_bound(name).is_some(), "{name}");
        }
        for name in ["select-1", "send-0", "stay-1", "escape", "rel-next-j", ""] {
            assert!(resolve_fixture_bound(name).is_none(), "{name}");
        }
    }

    #[test]
    fn transport_parity_with_product_chords() {
        // Shared directional chords post bit-identically to the helper
        // owned-post composition under the Ctrl role.
        for name in ["focus-h", "focus-left", "move-j", "move-right"] {
            let owned = crate::owned_control::resolve_chord(name).unwrap();
            let chord = resolve_fixture_chord(name).unwrap();
            let (mut want, _) = crate::input::cg_flag_bits(ProbeModifier::Ctrl);
            if owned.shift {
                want |= crate::native_input::CG_SHIFT_BIT;
            }
            if owned.alt {
                want |= crate::native_input::CG_ALT_BIT;
            }
            assert_eq!(fixture_flags(ProbeModifier::Ctrl, chord), want, "{name}");
        }
        // Numbered selects ride Ctrl alone; send-follows ride Ctrl+Shift
        // (exact product Desktop-N shapes, trailing 0 included).
        const CTRL: u64 = 0x0004_0000;
        const SHIFT: u64 = 0x0002_0000;
        for index in 0..=9u32 {
            let select = resolve_fixture_chord(&format!("select-{index}")).unwrap();
            assert_eq!(
                fixture_flags(ProbeModifier::Ctrl, select),
                CTRL,
                "select-{index}"
            );
            let send = resolve_fixture_chord(&format!("send-{index}")).unwrap();
            assert_eq!(
                fixture_flags(ProbeModifier::Ctrl, send),
                CTRL | SHIFT,
                "send-{index}"
            );
        }
        // Trailing select-0/send-0 carry the Desktop-10 digit keycode with
        // the same arms as the armed product catalogue rows.
        let zero_digit = crate::desktop_shortcuts::desktop_digit_keycode(10).unwrap();
        assert_eq!(
            resolve_fixture_chord("select-0").unwrap().key_code,
            zero_digit
        );
        assert_eq!(
            resolve_fixture_chord("send-0").unwrap().key_code,
            zero_digit
        );
        for (id, follow) in [("workspace-select-0", false), ("workspace-send-0", true)] {
            let row = crate::workspace_input::workspace_catalogue()
                .into_iter()
                .find(|row| row.id == id)
                .expect("trailing row exists");
            assert!(row.implemented, "{id}");
            let binding = row.binding().expect("trailing row binds");
            assert_eq!(binding.key_code, zero_digit, "{id} keycode parity");
            assert_eq!(
                crate::workspace_input::effective_flags_for(ProbeModifier::Ctrl, &binding),
                fixture_flags(
                    ProbeModifier::Ctrl,
                    resolve_fixture_chord(if follow { "send-0" } else { "select-0" }).unwrap()
                ),
                "{id} flags parity"
            );
        }
        // Bound relative/previous chords compose exactly the catalogue
        // effective flags (independent home, same tap tuning).
        for name in ["rel-prev", "rel-next", "prev-toggle"] {
            let binding = resolve_fixture_bound(name).unwrap();
            let chord = FixtureChord {
                key_code: binding.key_code,
                shift: binding.shift,
                alt: binding.alt,
                aux: binding.aux,
                action: "select",
            };
            assert_eq!(
                fixture_flags(ProbeModifier::Ctrl, chord),
                crate::workspace_input::effective_flags_for(ProbeModifier::Ctrl, &binding),
                "{name}"
            );
        }
    }

    #[test]
    fn unarmed_product_rows_stay_refused_with_catalogue_evidence() {
        // Numbered stays are bindable-unbound (no default chord): the
        // catalogue marks 1..9 implemented but carries no chord, so the
        // fixture vocabulary admits no stay transport (the TextEdit stay-1/2
        // diag injection is never a fixture route).
        for index in [1u8, 2, 9] {
            let id = format!("stay-workspace-{index}");
            let row = crate::workspace_input::workspace_catalogue()
                .into_iter()
                .find(|row| row.id == id)
                .expect("stay row exists");
            assert!(row.implemented, "{id}");
            assert!(row.binding().is_none(), "{id} carries no default chord");
        }
        assert!(resolve_fixture_chord("stay-1").is_none());
        // Trailing stay-0 is unimplemented end to end; relative sends/stays
        // are unimplemented catalogue rows: none is admittable here.
        let stay_0 = crate::workspace_input::workspace_catalogue()
            .into_iter()
            .find(|row| row.id == "stay-workspace-0")
            .expect("trailing stay row exists");
        assert!(!stay_0.implemented);
        // Relative sends/stays are unimplemented catalogue rows: the
        // catalogue carries no admittable relative-send/stay binding.
        let mut relative_send_rows = 0usize;
        for row in crate::workspace_input::workspace_catalogue() {
            if matches!(
                row.action,
                crate::runtime_types::InputSemanticAction::WorkspaceSendRelative { .. }
            ) {
                assert!(!row.implemented, "{}", row.id);
                relative_send_rows += 1;
            }
        }
        assert!(relative_send_rows > 0, "relative send rows exist");
        assert!(resolve_fixture_chord("send-rel-prev").is_none());
    }

    #[test]
    fn announcement_parses_harness_owned_shapes() {
        // Exact announcement binds; every shape drift refuses before any
        // live touch (no parked-scaffold parser involved).
        let parsed = parse_fixture_announcement(&announcement(NONCE)).expect("good parses");
        assert_eq!(parsed.pid, 4242);
        assert_eq!(parsed.socket, "/tmp/owned-fixture/fixture.sock");
        assert_eq!(parsed.nonce, NONCE);
        for bad in [
            "not json",
            "{\"socket\":\"\",\"pid\":1,\"nonce\":\"ab\"}",
            "{\"socket\":\"/s\",\"pid\":0,\"nonce\":\"ab\"}",
            "{\"socket\":\"/s\",\"pid\":1,\"nonce\":\"zz\"}",
            "{\"socket\":\"/s\",\"pid\":4294967296,\"nonce\":\"ab\"}",
        ] {
            assert!(parse_fixture_announcement(bad).is_err(), "{bad}");
        }
        // Socket path shapes refuse purely.
        assert!(socket_path_valid("/tmp/run/fixture.sock").is_ok());
        assert!(socket_path_valid("").is_err());
        assert!(socket_path_valid("/tmp/\0sock").is_err());
        assert!(socket_path_valid(&"x".repeat(105)).is_err());
    }

    #[test]
    fn title_join_parses_sentinel_caller_run_tag_and_compares() {
        assert_eq!(
            parse_fixture_title("OmniTilerTestWindow:a:abc123").unwrap(),
            ("a".to_owned(), "abc123".to_owned())
        );
        assert_eq!(
            parse_fixture_title("OmniTilerTestWindow:win-b:run9 trailing text").unwrap(),
            ("win-b".to_owned(), "run9".to_owned())
        );
        // Caller charset mirrors the fixture protocol ids.
        assert!(caller_id_valid("a"));
        assert!(caller_id_valid("win-b_2.C"));
        assert!(!caller_id_valid(""));
        assert!(!caller_id_valid("has space"));
        assert!(!caller_id_valid("has:colon"));
        assert!(!caller_id_valid(&"x".repeat(65)));
        // Foreign/malformed titles refuse.
        for bad in [
            "",
            "TextEdit",
            "OmniTilerTestWindow",
            "OmniTilerTestWindow:",
            "OmniTilerTestWindow::tag",
            "OmniTilerTestWindow:bad caller:tag",
            "OmniTilerTestWindow:a:",
            "OmniTilerTestWindow:a: ",
            "Ghostty:a:tag",
        ] {
            assert!(parse_fixture_title(bad).is_err(), "{bad:?}");
        }
        // The sentinel pins the exact fixture spelling.
        assert_eq!(FIXTURE_TITLE_SENTINEL, "OmniTilerTestWindow");
        // Run-tag comparison binds the incarnation across the join.
        assert!(run_tag_verdict("t1", "t1").is_ok());
        assert!(run_tag_verdict("t1", "t2").is_err());
        assert!(run_tag_verdict("", "t1").is_err());
    }

    #[test]
    fn socket_reply_parser_matches_real_wire_shapes() {
        // Exact live-shaped reply binds pid/caller/run-tag.
        let snap = parse_socket_get_reply(
            &socket_reply(4242, "OmniTilerTestWindow:a:run-live"),
            SOCKET_REQUEST_ID,
            "a",
        )
        .expect("live-shaped reply parses");
        assert_eq!(snap.pid, 4242);
        assert_eq!(snap.caller, "a");
        assert_eq!(snap.run_tag, "run-live");
        assert_eq!(snap.native_handle, "handle-live-1");
        // Reply-id mismatch refuses (never a cross-connection join).
        assert!(
            parse_socket_get_reply(
                &socket_reply(4242, "OmniTilerTestWindow:a:run-live"),
                "other-id",
                "a"
            )
            .is_err()
        );
        // Request-level refusal maps stale nonces precisely; server error
        // text never passes through (it can quote our secret-bearing
        // request).
        let bad_nonce = serde_json::json!({
            "v": 1, "request_id": SOCKET_REQUEST_ID,
            "ok": false, "error": "bad nonce",
        });
        let err = parse_socket_get_reply(&bad_nonce, SOCKET_REQUEST_ID, "a").unwrap_err();
        assert!(err.contains("nonce"), "unexpected: {err}");
        assert!(!err.contains("bad nonce"), "server text leaks: {err}");
        let refused = serde_json::json!({
            "v": 1, "request_id": SOCKET_REQUEST_ID,
            "ok": false, "error": "unknown op 'teleport'",
        });
        let err = parse_socket_get_reply(&refused, SOCKET_REQUEST_ID, "a").unwrap_err();
        assert!(!err.contains("teleport"), "server text leaks: {err}");
        // Unknown caller window refuses distinctly from request refusal.
        let unknown = serde_json::json!({
            "v": 1, "request_id": SOCKET_REQUEST_ID, "ok": false,
            "result": {"windows": [
                {"id": "a", "ok": false, "error": "unknown window id", "fields": []}
            ]},
        });
        let err = parse_socket_get_reply(&unknown, SOCKET_REQUEST_ID, "a").unwrap_err();
        assert!(err.contains("unknown"), "unexpected: {err}");
        // Missing entry, missing snapshot, unreadable pid/title refuse.
        let no_entry = serde_json::json!({
            "v": 1, "request_id": SOCKET_REQUEST_ID, "ok": true,
            "result": {"windows": []},
        });
        assert!(parse_socket_get_reply(&no_entry, SOCKET_REQUEST_ID, "a").is_err());
        let no_snapshot = serde_json::json!({
            "v": 1, "request_id": SOCKET_REQUEST_ID, "ok": true,
            "result": {"windows": [{"id": "a", "ok": true, "fields": []}]},
        });
        assert!(parse_socket_get_reply(&no_snapshot, SOCKET_REQUEST_ID, "a").is_err());
        let bad_pid = socket_reply(4242, "OmniTilerTestWindow:a:run-live");
        let mut bad_pid = bad_pid;
        bad_pid["result"]["windows"][0]["snapshot"]["pid"] = serde_json::json!("4242");
        assert!(parse_socket_get_reply(&bad_pid, SOCKET_REQUEST_ID, "a").is_err());
        // Socket caller mismatch refuses (server window is not ours).
        assert!(
            parse_socket_get_reply(
                &socket_reply(4242, "OmniTilerTestWindow:b:run-live"),
                SOCKET_REQUEST_ID,
                "a"
            )
            .is_err()
        );
        // The opaque handle binds the window incarnation: a missing or
        // empty handle refuses (never compared across domains, only
        // retained for stability - but absence means no live window).
        let mut no_handle = socket_reply(4242, "OmniTilerTestWindow:a:run-live");
        no_handle["result"]["windows"][0]["snapshot"]
            .as_object_mut()
            .expect("snapshot object")
            .remove("native_handle");
        assert!(parse_socket_get_reply(&no_handle, SOCKET_REQUEST_ID, "a").is_err());
        let empty_handle = socket_reply_handle(4242, "OmniTilerTestWindow:a:run-live", "");
        assert!(parse_socket_get_reply(&empty_handle, SOCKET_REQUEST_ID, "a").is_err());
        // A re-created window carries a fresh handle under the same
        // run-wide tag: the parser surfaces it so the driver can refuse
        // the incarnation drift across calls.
        let recreated =
            socket_reply_handle(4242, "OmniTilerTestWindow:a:run-live", "handle-live-2");
        let snap = parse_socket_get_reply(&recreated, SOCKET_REQUEST_ID, "a").expect("parses");
        assert_eq!(snap.run_tag, "run-live");
        assert_eq!(snap.native_handle, "handle-live-2");
    }

    #[test]
    fn secure_input_unknown_refuses_for_this_harness() {
        // Stricter than the legacy helper/TextEdit gates (which inherit a
        // permissive unknown): only observed-inactive proceeds here.
        assert!(secure_input_verdict(Some(false)).is_ok());
        let active = secure_input_verdict(Some(true)).unwrap_err();
        assert!(active.contains("active"), "unexpected: {active}");
        let unknown = secure_input_verdict(None).unwrap_err();
        assert!(unknown.contains("unknown"), "unexpected: {unknown}");
    }

    #[test]
    fn incarnation_binds_once_then_demands_stability() {
        let first = WindowIncarnation {
            run_tag: "run-live".to_owned(),
            native_handle: "handle-live-1".to_owned(),
        };
        // First proof binds: anything (including nothing) admits.
        assert!(incarnation_verdict(None, &first).is_ok());
        assert!(incarnation_verdict(Some(&first), &first).is_ok());
        // Same run-wide tag but a fresh handle (re-created caller window)
        // refuses without echoing the values.
        let recreated = WindowIncarnation {
            run_tag: "run-live".to_owned(),
            native_handle: "handle-live-2".to_owned(),
        };
        let err = incarnation_verdict(Some(&first), &recreated).unwrap_err();
        assert!(err.contains("incarnation drift"), "unexpected: {err}");
        assert!(!err.contains("handle-live"), "leak: {err}");
        // A run-tag change refuses too.
        let retagged = WindowIncarnation {
            run_tag: "run-other".to_owned(),
            native_handle: "handle-live-1".to_owned(),
        };
        assert!(incarnation_verdict(Some(&first), &retagged).is_err());
    }

    #[test]
    fn designated_tail_extracts_exactly() {
        // Real `codesign -d -r -` shape (stderr): the `Executable=` line plus
        // the requirement line. The combined stdout+stderr output carries
        // both; only the requirement line compares. No machine paths below:
        // synthetic fixture values only.
        let stderr_first = "Executable=/fixture/OmniTilerLiveFixture\nIdentifier=io.github.omnitiler.live-fixture\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20400 size=1\nDesignated Requirement: designated => identifier \"x\" and anchor apple generic\n";
        assert_eq!(
            extract_designated_tail(stderr_first).unwrap(),
            "designated => identifier \"x\" and anchor apple generic"
        );
        // Stream order reversed (requirement first, executable trailing):
        // the trailing line is still excluded.
        let stdout_first = "Designated Requirement: designated => identifier \"x\" and anchor apple generic\nExecutable=/fixture/OmniTilerLiveFixture\n";
        assert_eq!(
            extract_designated_tail(stdout_first).unwrap(),
            "designated => identifier \"x\" and anchor apple generic"
        );
        // Requirement with trailing blank lines and CRLF endings.
        let padded = "\nExecutable=/fixture/OmniTilerLiveFixture\r\ndesignated => identifier \"x\" and anchor apple generic\r\n\r\n";
        assert_eq!(
            extract_designated_tail(padded).unwrap(),
            "designated => identifier \"x\" and anchor apple generic"
        );
        // A bare requirement line with no surrounding output.
        assert_eq!(
            extract_designated_tail("designated => identifier \"x\"").unwrap(),
            "designated => identifier \"x\""
        );
        assert!(extract_designated_tail("no marker here").is_err());
        assert!(extract_designated_tail("").is_err());
        assert!(extract_designated_tail("Executable=/fixture/OmniTilerLiveFixture\n").is_err());
    }

    #[test]
    fn ownership_verdicts_refuse_foreign_protected_and_stale_targets() {
        // Bundle gate: exactly the signed fixture scope passes (signature
        // proof itself rides the codesign check, never this string).
        assert!(bundle_verdict(Some(FIXTURE_BUNDLE)).is_ok());
        assert!(bundle_verdict(Some("com.mitchellh.ghostty")).is_err());
        assert!(bundle_verdict(Some("COM.MITCHELLH.GHOSTTY")).is_err());
        assert!(bundle_verdict(Some("com.apple.Terminal")).is_err());
        assert!(bundle_verdict(Some(crate::BUNDLE_ID)).is_err());
        assert!(bundle_verdict(Some("com.apple.TextEdit")).is_err());
        assert!(bundle_verdict(Some("")).is_err());
        assert!(bundle_verdict(None).is_err());
        // Frontmost gate: exactly the fixture PID passes; anything else -
        // including the controller itself - refuses as foreign.
        assert!(front_verdict(Some(4242), 4242).is_ok());
        assert!(front_verdict(Some(555), 4242).is_err());
        assert!(front_verdict(Some(std::process::id() as i32), 4242).is_err());
        assert!(front_verdict(None, 4242).is_err());
        // Lifetime gate: exact equality only; drift or unreadable refuses
        // without echoing the values.
        assert!(lifetime_verdict(Some("/x"), "/x", Some("l1"), "l1").is_ok());
        for (exe, bound_exe, launch, bound_launch) in [
            (Some("/y"), "/x", Some("l1"), "l1"),
            (Some("/x"), "/x", Some("l2"), "l1"),
            (None, "/x", Some("l1"), "l1"),
            (Some("/x"), "/x", None, "l1"),
        ] {
            let err = lifetime_verdict(exe, bound_exe, launch, bound_launch).unwrap_err();
            assert!(!err.contains("/y") && !err.contains("/x"), "leak: {err}");
            assert!(!err.contains("l1") && !err.contains("l2"), "leak: {err}");
        }
    }

    #[test]
    fn full_check_orders_gates_and_fails_first_wins() {
        let sess = good_session();
        // Exact full proof passes and surfaces the snapshot plus the
        // candidate window incarnation. Foreground and oracle are not full
        // gates: the commit owns foreground (last) and the selected step
        // owns attribution (after every blocking read).
        let (snap, incarnation) =
            check_full_reads(&sess, "a", 7, &good_full()).expect("exact proof passes");
        assert_eq!(snap.pid, 4242);
        assert_eq!(snap.caller, "a");
        assert_eq!(snap.run_tag, "run-live");
        assert_eq!(snap.native_handle, "handle-live-1");
        assert_eq!(incarnation.run_tag, "run-live");
        assert_eq!(incarnation.native_handle, "handle-live-1");
        // Each drifted leg refuses with its own precise stage message.
        let mut stale_nonce = good_full();
        stale_nonce.socket_reply = serde_json::json!({
            "v": 1, "request_id": SOCKET_REQUEST_ID,
            "ok": false, "error": "bad nonce",
        });
        let err = check_full_reads(&sess, "a", 7, &stale_nonce).unwrap_err();
        assert!(err.contains("nonce"), "unexpected: {err}");
        let mut pid_reuse = good_full();
        pid_reuse.socket_reply = socket_reply(7777, "OmniTilerTestWindow:a:run-live");
        let err = check_full_reads(&sess, "a", 7, &pid_reuse).unwrap_err();
        assert!(
            err.contains("7777") && err.contains("4242"),
            "unexpected: {err}"
        );
        let mut foreign_bundle = good_full();
        foreign_bundle.bundle = Some("com.apple.TextEdit".to_owned());
        let err = check_full_reads(&sess, "a", 7, &foreign_bundle).unwrap_err();
        assert!(err.contains("TextEdit"), "unexpected: {err}");
        let mut exe_drift = good_full();
        exe_drift.exe = Some("/other-exe".to_owned());
        let err = check_full_reads(&sess, "a", 7, &exe_drift).unwrap_err();
        assert!(err.contains("executable drift"), "unexpected: {err}");
        assert!(!err.contains("/other-exe"), "leak: {err}");
        let mut wid_gone = good_full();
        wid_gone.cg_entry = None;
        let err = check_full_reads(&sess, "a", 7, &wid_gone).unwrap_err();
        assert!(
            err.contains("not present in CGWindowList"),
            "unexpected: {err}"
        );
        let mut wid_foreign = good_full();
        wid_foreign.cg_entry = Some((9999, Some("OmniTilerTestWindow:a:run-live".to_owned())));
        let err = check_full_reads(&sess, "a", 7, &wid_foreign).unwrap_err();
        assert!(err.contains("belongs to pid 9999"), "unexpected: {err}");
        let mut title_drift = good_full();
        title_drift.cg_entry = Some((4242, Some("OmniTilerTestWindow:b:run-live".to_owned())));
        let err = check_full_reads(&sess, "a", 7, &title_drift).unwrap_err();
        assert!(err.contains("caller mismatch"), "unexpected: {err}");
        // Run-tag incarnation drift: socket and native disagree.
        let mut retag = good_full();
        retag.cg_entry = Some((4242, Some("OmniTilerTestWindow:a:run-other".to_owned())));
        let err = check_full_reads(&sess, "a", 7, &retag).unwrap_err();
        assert!(err.contains("run tag"), "unexpected: {err}");
        let mut ax_lost = good_full();
        ax_lost.ax_contains = false;
        let err = check_full_reads(&sess, "a", 7, &ax_lost).unwrap_err();
        assert!(err.contains("AX windows"), "unexpected: {err}");
        // The full proof carries no foreground, oracle, or signature
        // verdict: the commit, the selected step, and the bind own them.
        // Ordering: socket beats bundle, bundle beats CG, CG beats AX.
        let mut two = good_full();
        two.socket_reply = stale_nonce.socket_reply.clone();
        two.bundle = Some("com.apple.TextEdit".to_owned());
        let err = check_full_reads(&sess, "a", 7, &two).unwrap_err();
        assert!(err.contains("nonce"), "socket must win: {err}");
        let mut two = good_full();
        two.bundle = Some("com.apple.TextEdit".to_owned());
        two.cg_entry = None;
        let err = check_full_reads(&sess, "a", 7, &two).unwrap_err();
        assert!(err.contains("TextEdit"), "bundle must win: {err}");
        let mut two = good_full();
        two.cg_entry = None;
        two.ax_contains = false;
        let err = check_full_reads(&sess, "a", 7, &two).unwrap_err();
        assert!(err.contains("CGWindowList"), "CG must win: {err}");
    }

    #[test]
    fn commit_check_is_continuity_then_target_then_foreground_last() {
        let sess = good_session();
        let incarnation = bound_incarnation();
        // Exact commit passes on the post path and on the focus path.
        assert!(check_commit_reads(&sess, "a", 7, &good_commit(), &incarnation, true).is_ok());
        assert!(check_commit_reads(&sess, "a", 7, &good_commit(), &incarnation, false).is_ok());
        // Process continuity first: executable or launch drift refuses
        // without echoing values, before any target read is judged.
        let mut exe_drift = good_commit();
        exe_drift.exe = Some("/other-exe".to_owned());
        let err = check_commit_reads(&sess, "a", 7, &exe_drift, &incarnation, true).unwrap_err();
        assert!(err.contains("executable drift"), "unexpected: {err}");
        assert!(!err.contains("/other-exe"), "leak: {err}");
        let mut launch_drift = good_commit();
        launch_drift.launch = Some("launch-other".to_owned());
        let err = check_commit_reads(&sess, "a", 7, &launch_drift, &incarnation, true).unwrap_err();
        assert!(err.contains("launch date drift"), "unexpected: {err}");
        // Target: missing, foreign, wrong-caller, and retagged windows
        // refuse before the foreground gate.
        let mut wid_gone = good_commit();
        wid_gone.cg_entry = None;
        let err = check_commit_reads(&sess, "a", 7, &wid_gone, &incarnation, true).unwrap_err();
        assert!(
            err.contains("not present in CGWindowList"),
            "unexpected: {err}"
        );
        let mut wid_foreign = good_commit();
        wid_foreign.cg_entry = Some((9999, Some("OmniTilerTestWindow:a:run-live".to_owned())));
        let err = check_commit_reads(&sess, "a", 7, &wid_foreign, &incarnation, true).unwrap_err();
        assert!(err.contains("belongs to pid 9999"), "unexpected: {err}");
        let mut title_drift = good_commit();
        title_drift.cg_entry = Some((4242, Some("OmniTilerTestWindow:b:run-live".to_owned())));
        let err = check_commit_reads(&sess, "a", 7, &title_drift, &incarnation, true).unwrap_err();
        assert!(err.contains("caller mismatch"), "unexpected: {err}");
        let mut retag = good_commit();
        retag.cg_entry = Some((4242, Some("OmniTilerTestWindow:a:run-other".to_owned())));
        let err = check_commit_reads(&sess, "a", 7, &retag, &incarnation, true).unwrap_err();
        assert!(err.contains("incarnation"), "unexpected: {err}");
        // Foreground LAST: a foreign frontmost (including the hosting
        // terminal's PID) refuses, and the focus path skips it.
        let mut foreign_front = good_commit();
        foreign_front.front_pid = Some(555);
        let err =
            check_commit_reads(&sess, "a", 7, &foreign_front, &incarnation, true).unwrap_err();
        assert!(err.contains("frontmost pid 555"), "unexpected: {err}");
        assert!(check_commit_reads(&sess, "a", 7, &foreign_front, &incarnation, false).is_ok());
        // Ordering: continuity beats target, target beats foreground.
        let mut two = good_commit();
        two.exe = Some("/other-exe".to_owned());
        two.cg_entry = None;
        let err = check_commit_reads(&sess, "a", 7, &two, &incarnation, true).unwrap_err();
        assert!(
            err.contains("executable drift"),
            "continuity must win: {err}"
        );
        let mut two = good_commit();
        two.cg_entry = None;
        two.front_pid = Some(555);
        let err = check_commit_reads(&sess, "a", 7, &two, &incarnation, true).unwrap_err();
        assert!(
            err.contains("CGWindowList"),
            "target must win over foreground: {err}"
        );
    }

    #[test]
    fn bind_refuses_precisely_before_any_post() {
        // Usage shapes exit 2 without touching the transport.
        let transport = ScriptTransport::passing();
        assert_eq!(
            FixtureDriver::bind(&transport, "not json", NONCE, SIGNER, test_admission())
                .unwrap_err()
                .0,
            2
        );
        assert_eq!(
            FixtureDriver::bind(
                &transport,
                &announcement(NONCE),
                &"cd".repeat(32),
                SIGNER,
                test_admission()
            )
            .unwrap_err()
            .0,
            2
        );
        assert_eq!(
            FixtureDriver::bind(
                &transport,
                &announcement(NONCE),
                NONCE,
                "  ",
                test_admission()
            )
            .unwrap_err()
            .0,
            2
        );
        // Session-gate failure exits 1 at stage session with zero posts.
        let mut gated = ScriptTransport::passing();
        gated.gates_error = Some("slps-unavailable: gone".to_owned());
        let err = FixtureDriver::bind(
            &gated,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"session\""),
            "unexpected: {}",
            err.1
        );
        assert!(gated.posts.borrow().is_empty());
        // Identity drift exits 1 at stage identity.
        let mut foreign = ScriptTransport::passing();
        foreign.identity = Ok(IdentityReads {
            bundle: Some("com.apple.TextEdit".to_owned()),
            exe: Some("/live-exe".to_owned()),
            launch: Some("launch-live".to_owned()),
        });
        let err = FixtureDriver::bind(
            &foreign,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"identity\""),
            "unexpected: {}",
            err.1
        );
        assert!(foreign.posts.borrow().is_empty());
        // Signature drift exits 1 at stage signature.
        let mut resigned = ScriptTransport::passing();
        resigned.designated = Ok("designated => identifier \"evil\"".to_owned());
        let err = FixtureDriver::bind(
            &resigned,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"signature\""),
            "unexpected: {}",
            err.1
        );
        assert!(!err.1.contains("evil"), "leak: {}", err.1);
        assert!(resigned.posts.borrow().is_empty());
    }

    #[test]
    fn bind_refuses_foreign_admission_before_any_post() {
        // A leg admission bound to another fixture run refuses at bind
        // (exit 2, stale or foreign admission), before any gate or post.
        // This is the executable answer to the legacy bypass: without a
        // fresh leg there is no admission that binds.
        let transport = ScriptTransport::passing();
        let foreign =
            LegAdmission::admission_for_test(7777, crate::live_leg::AdmissionScope::SpaceMutating);
        let err = FixtureDriver::bind(&transport, &announcement(NONCE), NONCE, SIGNER, foreign)
            .unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("admission"), "unexpected: {}", err.1);
        assert!(transport.posts.borrow().is_empty());
        assert_eq!(*transport.designated_calls.borrow(), 0);
    }

    #[test]
    fn post_only_admission_refuses_space_mutation() {
        // A post-only admission (directional leg without the executed
        // return probe) runs focus/move chords but refuses selects, sends,
        // and bound relative chords before any proof or post.
        let transport = ScriptTransport::passing();
        let post_only =
            LegAdmission::admission_for_test(4242, crate::live_leg::AdmissionScope::PostOnly);
        let driver =
            FixtureDriver::bind(&transport, &announcement(NONCE), NONCE, SIGNER, post_only)
                .expect("binds");
        driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .expect("directional posts");
        assert_eq!(transport.posts.borrow().len(), 2);
        for chord in ["select-1", "send-2", "rel-prev", "prev-toggle"] {
            let err = driver.post("a", 7, chord, ProbeModifier::Ctrl).unwrap_err();
            assert_eq!(err.0, 2, "{chord}");
            assert!(err.1.contains("Space"), "{chord}: {}", err.1);
        }
        // No further posts after the first directional pair.
        assert_eq!(transport.posts.borrow().len(), 2);
    }

    #[test]
    fn revoked_admission_refuses_every_entry() {
        // A revoked admission (leg finished) refuses at bind, at post, and
        // at focus alike: replay across legs or after finish is impossible.
        let transport = ScriptTransport::passing();
        let dead = test_admission();
        dead.revoke_for_test();
        let err = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            dead.clone(),
        )
        .unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("revoked"), "unexpected: {}", err.1);
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        // Revoke after bind: the held driver copy shares the token, so the
        // next post and focus refuse before any proof or post.
        let live = test_admission();
        let driver2 = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            live.clone(),
        )
        .expect("binds");
        live.revoke_for_test();
        let err = driver2
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(err.1.contains("revoked"), "unexpected: {}", err.1);
        let err = driver2.focus("a", 7).unwrap_err();
        assert_eq!(err.0, 1);
        assert!(err.1.contains("revoked"), "unexpected: {}", err.1);
        assert!(transport.posts.borrow().is_empty());
        let _ = driver;
    }

    #[test]
    fn post_success_posts_down_then_up_with_exact_chord() {
        let transport = ScriptTransport::passing();
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let report = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .expect("posts");
        let value: serde_json::Value = serde_json::from_str(&report).expect("report parses");
        assert_eq!(value["outcome"], "posted");
        assert_eq!(value["pid"], 4242);
        assert_eq!(value["wid"], 7);
        assert_eq!(value["chord"], "focus-h");
        assert_eq!(value["released_bypass"], false);
        assert!(value.get("down_proof_us").is_some());
        assert!(value.get("up_proof_us").is_some());
        // Exactly two posts: down then up, same key/flags, cleared never
        // (no bypass on the clean path).
        assert_eq!(
            *transport.posts.borrow(),
            vec![
                (4242, 0x04, 0x0004_0000, true),
                (4242, 0x04, 0x0004_0000, false),
            ]
        );
        // Blocking authentication stays outside the held-key pair: one
        // bind-time signature read, one full proof, one exact selected
        // attribution, two cheap commits, and no second full proof for
        // the up edge. The live-leg order pins full -> selected ->
        // commit -> down -> commit -> up.
        assert_eq!(*transport.designated_calls.borrow(), 1);
        assert_eq!(*transport.full_calls.borrow(), 1);
        assert_eq!(*transport.selected_calls.borrow(), 1);
        assert_eq!(*transport.commit_calls.borrow(), 2);
        assert_eq!(
            *transport.order.borrow(),
            vec![
                "full",
                "selected",
                "commit",
                "post-down",
                "commit",
                "post-up",
            ]
        );
    }

    #[test]
    fn selected_loss_after_full_refuses_before_any_post() {
        // Same-PID focus move to another fixture window after the full
        // proof: the app is frontmost and the target wid still exists, so
        // the cheap commit would pass - but the exact selected attribution
        // sampled after the blocking reads refuses at proof-down with
        // nothing posted and no commit reached.
        let mut moved = ScriptTransport::passing();
        moved.selected = Err(
            "refusing fixture post: selected oracle disagrees on pid 4242 wid 7; refusing"
                .to_owned(),
        );
        let driver = FixtureDriver::bind(
            &moved,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"proof-down\""),
            "unexpected: {}",
            err.1
        );
        assert!(err.1.contains("oracle disagrees"), "unexpected: {}", err.1);
        assert!(moved.posts.borrow().is_empty());
        assert_eq!(*moved.order.borrow(), vec!["full", "selected",]);
        assert_eq!(*moved.commit_calls.borrow(), 0);
    }

    #[test]
    fn post_refusals_happen_before_any_post() {
        // Proof failure on the down edge: stage proof-down, nothing posted.
        let mut failing = ScriptTransport::passing();
        failing.full_first = Err("full-reads: socket: connect refused (injected)".to_owned());
        let driver = FixtureDriver::bind(
            &failing,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"proof-down\""),
            "unexpected: {}",
            err.1
        );
        assert!(err.1.contains("connect refused"), "unexpected: {}", err.1);
        assert!(failing.posts.borrow().is_empty());
        // Unknown chord refuses with code 2 before any proof or post.
        let transport = ScriptTransport::passing();
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "stay-1", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 2);
        assert!(transport.posts.borrow().is_empty());
        // Malformed caller/wid refuse with code 2 before any post.
        let err = driver
            .post("bad caller", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 2);
        let err = driver
            .post("a", 0, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 2);
        assert!(transport.posts.borrow().is_empty());
        // Semantic proof drift (wrong caller) refuses at proof-down
        // before any post.
        let drifting = ScriptTransport::passing();
        let driver = FixtureDriver::bind(
            &drifting,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("b", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"proof-down\""),
            "unexpected: {}",
            err.1
        );
        assert!(drifting.posts.borrow().is_empty());
    }

    #[test]
    fn up_commit_loss_after_down_releases_exactly_once_cleared() {
        // Clean full proof and down commit, then a foreign foreground on
        // the held-key up commit: exactly one bypass release (key-up,
        // flags cleared), stage proof-up, and the gated up post never
        // runs. No second full proof and no signature fork run while the
        // key is held.
        let mut transport = ScriptTransport::passing();
        let mut drifted = good_commit();
        drifted.front_pid = Some(555);
        transport.commit_rest = Some(Ok(drifted));
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"proof-up\""),
            "unexpected: {}",
            err.1
        );
        assert!(err.1.contains("frontmost pid 555"), "unexpected: {}", err.1);
        assert!(
            err.1.contains("released_bypass\":true"),
            "unexpected: {}",
            err.1
        );
        assert_eq!(
            *transport.posts.borrow(),
            vec![(4242, 0x04, 0x0004_0000, true), (4242, 0x04, 0, false),]
        );
        assert_eq!(*transport.designated_calls.borrow(), 1);
        assert_eq!(*transport.full_calls.borrow(), 1);
        // The up edge is release continuity, never a second attribution:
        // one selected sample total even though the commit ran twice, and
        // the bypass key-up closes the order.
        assert_eq!(*transport.selected_calls.borrow(), 1);
        assert_eq!(*transport.commit_calls.borrow(), 2);
        assert_eq!(
            *transport.order.borrow(),
            vec![
                "full",
                "selected",
                "commit",
                "post-down",
                "commit",
                "post-up",
            ]
        );
    }

    #[test]
    fn up_commit_target_drift_releases_and_never_posts_up() {
        // Retagged target on the held-key commit: the commit's CG join
        // refuses against the bound incarnation, the held key is released
        // unconditionally, and no gated up runs.
        let mut transport = ScriptTransport::passing();
        let mut drifted = good_commit();
        drifted.cg_entry = Some((4242, Some("OmniTilerTestWindow:a:run-other".to_owned())));
        transport.commit_rest = Some(Ok(drifted));
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"proof-up\""),
            "unexpected: {}",
            err.1
        );
        assert!(err.1.contains("incarnation"), "unexpected: {}", err.1);
        assert!(
            err.1.contains("released_bypass\":true"),
            "unexpected: {}",
            err.1
        );
        assert_eq!(
            *transport.posts.borrow(),
            vec![(4242, 0x04, 0x0004_0000, true), (4242, 0x04, 0, false),]
        );
    }

    #[test]
    fn bypass_failure_is_explicit_non_evidence_exactly_once() {
        // Commit loss plus a failing bypass attempt: stage proof-up,
        // `released_bypass:false` with the release cause surfaced, exactly
        // two posts (down plus the one failed bypass attempt - no retries),
        // and the guard disarmed so no second release can follow.
        let mut transport = ScriptTransport::passing();
        let mut drifted = good_commit();
        drifted.front_pid = Some(555);
        transport.commit_rest = Some(Ok(drifted));
        transport.fail_bypass_post = true;
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"proof-up\""),
            "unexpected: {}",
            err.1
        );
        assert!(
            err.1.contains("released_bypass\":false"),
            "unexpected: {}",
            err.1
        );
        assert!(
            err.1.contains("injected bypass-post failure"),
            "unexpected: {}",
            err.1
        );
        assert_eq!(
            *transport.posts.borrow(),
            vec![(4242, 0x04, 0x0004_0000, true), (4242, 0x04, 0, false),]
        );
    }

    #[test]
    fn commit_before_full_proof_refuses_without_posts() {
        // A commit with no bound incarnation is a driver logic error and
        // refuses before the window server is touched.
        let transport = ScriptTransport::passing();
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .prove_commit("a", 7, "commit-test", true)
            .unwrap_err();
        assert!(
            err.contains("no bound window incarnation"),
            "unexpected: {err}"
        );
        assert!(transport.posts.borrow().is_empty());
    }

    #[test]
    fn repeat_posts_share_one_session_with_no_signature_forks() {
        // Two sequential posts on one driver reuse the bound session and
        // guard continuity: exactly one bind-time signature read, one full
        // proof and two commits per post, four posts total, no intentional
        // delay between them (burst-capable by construction).
        let transport = ScriptTransport::passing();
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .expect("first posts");
        driver
            .post("a", 7, "select-1", ProbeModifier::Ctrl)
            .expect("second posts");
        assert_eq!(*transport.designated_calls.borrow(), 1);
        assert_eq!(*transport.full_calls.borrow(), 2);
        assert_eq!(*transport.selected_calls.borrow(), 2);
        assert_eq!(*transport.commit_calls.borrow(), 4);
        assert_eq!(transport.posts.borrow().len(), 4);
        // A re-created caller window (fresh opaque handle, same run-wide
        // tag) refuses the next full proof before any post: the bound
        // incarnation is stable across calls.
        let mut recreated = good_full();
        recreated.socket_reply =
            socket_reply_handle(4242, "OmniTilerTestWindow:a:run-live", "handle-live-2");
        let mut transport2 = ScriptTransport::passing();
        transport2.full_rest = Some(Ok(recreated));
        let driver2 = FixtureDriver::bind(
            &transport2,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        driver2
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .expect("first binds the incarnation");
        let posts_before = transport2.posts.borrow().len();
        let err = driver2
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"proof-down\""),
            "unexpected: {}",
            err.1
        );
        assert!(err.1.contains("incarnation drift"), "unexpected: {}", err.1);
        // No new posts on the refused full proof (the two from the first
        // post stand; nothing held, so no bypass either).
        assert_eq!(transport2.posts.borrow().len(), posts_before);
    }

    #[test]
    fn up_post_failure_releases_exactly_once_cleared() {
        // Gated up post fails after a posted down: exactly one bypass
        // release with cleared flags, stage post-up.
        let mut transport = ScriptTransport::passing();
        transport.fail_up_post = true;
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"post-up\""),
            "unexpected: {}",
            err.1
        );
        assert!(
            err.1.contains("released_bypass\":true"),
            "unexpected: {}",
            err.1
        );
        // The gated up attempt itself is counted (and failed); the bypass
        // follows it exactly once with cleared flags.
        assert_eq!(
            *transport.posts.borrow(),
            vec![
                (4242, 0x04, 0x0004_0000, true),
                (4242, 0x04, 0x0004_0000, false),
                (4242, 0x04, 0, false),
            ]
        );
    }

    #[test]
    fn interrupted_after_down_releases_exactly_once_cleared() {
        // Interrupt arriving after the down post: exactly one bypass
        // release with cleared flags, stage interrupted.
        let mut transport = ScriptTransport::passing();
        transport.latch_interrupt_after_down = true;
        let driver = FixtureDriver::bind(
            &transport,
            &announcement(NONCE),
            NONCE,
            SIGNER,
            test_admission(),
        )
        .expect("binds");
        let err = driver
            .post("a", 7, "focus-h", ProbeModifier::Ctrl)
            .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"interrupted\""),
            "unexpected: {}",
            err.1
        );
        // A key was held: exactly one cleared-flags bypass release.
        assert!(
            err.1.contains("released_bypass\":true"),
            "unexpected: {}",
            err.1
        );
        assert_eq!(
            *transport.posts.borrow(),
            vec![(4242, 0x04, 0x0004_0000, true), (4242, 0x04, 0, false),]
        );
    }

    #[test]
    fn bypass_release_posts_cleared_key_up_exactly_once() {
        // The reused guard primitive our post path depends on: gateless
        // key-up with flags cleared to zero, idempotent (exactly one up per
        // guard), inert unless a down posted. Pinned here so a change in the
        // shared primitive breaks this harness loudly.
        let posted = RefCell::new(Vec::<(u16, u64, bool)>::new());
        let mut guard = crate::textedit_post::EdgeGuard::new(4242, 0x04);
        assert!(guard.release_with(|_, _, _| Ok(99)).is_none());
        guard.note_down_posted();
        let first = guard
            .release_with(|code, flags, down| {
                posted.borrow_mut().push((code, flags, down));
                Ok(7)
            })
            .expect("pending guard releases");
        assert_eq!(first, 7);
        assert_eq!(*posted.borrow(), vec![(0x04, 0, false)]);
        assert!(guard.release_with(|_, _, _| Ok(8)).is_none());
        guard.note_released();
        assert!(guard.release_with(|_, _, _| Ok(9)).is_none());
    }

    #[test]
    fn post_path_owns_release_without_settle_sleep() {
        // Structural contract: the production path arms the pending-key
        // guard, checks the latched interrupt at explicit sequence points,
        // posts down/up through the injected transport, proves full before
        // anything is held and commits cheaply before each input, and
        // carries no settle sleep (pacing belongs to a future runner).
        // Scope to the production section so this test's own literals
        // cannot self-match.
        let source = include_str!("fixture_post.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for marker in [
            "EdgeGuard::new",
            "note_down_posted",
            "bypass_release",
            "note_released",
            "InterruptArmed::arm",
            "transport.interrupted",
            "transport.post_key",
            "prove_full",
            "prove_selected",
            "prove_commit",
            "selected_agreement",
            "FixtureDriver",
            "incarnation_verdict",
            "LegAdmission",
            "required_scope",
            "covers",
            "run_child_bounded",
            "try_wait",
        ] {
            assert!(production.contains(marker), "missing {marker}");
        }
        assert!(
            production.matches("prove_full").count() >= 3,
            "full proof definition plus down and focus call sites"
        );
        assert!(
            production.matches("prove_commit").count() >= 4,
            "commit definition plus down, up, and focus call sites"
        );
        // Down-path order is pinned structurally as well as behaviorally:
        // full proof, then the exact selected sample, then the cheap
        // commit, then the down post.
        let post_fn = production
            .split("pub fn post(")
            .nth(1)
            .expect("post fn")
            .split("pub fn focus(")
            .next()
            .unwrap_or("");
        for marker in ["prove_full", "prove_selected", "prove_commit"] {
            assert!(post_fn.contains(marker), "down path calls {marker}");
        }
        let full_at = post_fn.find("prove_full").unwrap_or(usize::MAX);
        let selected_at = post_fn.find("prove_selected").unwrap_or(usize::MAX);
        let commit_at = post_fn.find("prove_commit").unwrap_or(usize::MAX);
        let down_at = post_fn
            .find("post_key(pid, key_code, want_flags, true)")
            .unwrap_or(usize::MAX);
        assert!(
            full_at < selected_at && selected_at < commit_at && commit_at < down_at,
            "down order is full -> selected -> commit -> post"
        );
        // Foreground acquisition never samples attribution: the focus
        // region (through the CLI entry) calls no selected step.
        let focus_region = production
            .split("pub fn focus(")
            .nth(1)
            .expect("focus fn")
            .split("pub fn run_fixture_post(")
            .next()
            .unwrap_or("");
        assert!(
            !focus_region.contains("prove_selected")
                && !focus_region.contains("selected_agreement"),
            "focus acquisition skips the selected requirement"
        );
        // The only `thread::sleep` in production is the bounded 5 ms
        // owned-child exit poll: the driver region (everything from the
        // driver struct on) sleeps never.
        let driver_region = production
            .split("pub(crate) struct FixtureDriver")
            .nth(1)
            .expect("driver region");
        assert!(
            !driver_region.contains("thread::sleep") && !driver_region.contains("sleep("),
            "no settle sleep on the harness path"
        );
        assert!(
            production.contains("Duration::from_millis(5)"),
            "bounded child poll step stays explicit"
        );
        // Owned-child cleanup is kill-then-reap on every path: the
        // abandoned-`.output()` pattern is gone.
        assert!(
            production.contains("child.kill()"),
            "timeout kills the child"
        );
        assert!(
            production.contains("child.wait()"),
            "kill is always followed by reap"
        );
        // The abandoned-child pattern (a worker running `.output()` past
        // a channel timeout) is gone: the signing path owns its Child.
        assert!(
            !production.contains(".output()"),
            "no abandoned output worker"
        );
        assert!(
            !production.contains("fixture-codesign"),
            "no orphaned codesign worker thread"
        );
        assert!(
            !production.contains("post_guarded_with_tag"),
            "no tagged bypass on the fixture-post path"
        );
        assert!(
            !production.contains("world_stress_native"),
            "no parked-scaffold dependency in the harness"
        );
        assert!(
            !production.contains("fn edge_reads") && !production.contains("check_edge_reads"),
            "single-edge re-proof is replaced by the full/commit split"
        );
        // Signing identity is bind-only: exactly one transport call site
        // (bind), none on any edge path.
        assert_eq!(
            production.matches("transport.designated_tail").count(),
            1,
            "codesign read runs at bind only"
        );
        assert_eq!(
            production.matches("self.designated_tail").count(),
            0,
            "no edge path re-forks signing"
        );
        // Held-key commit stays cheap by construction: no socket, no AX
        // enumeration, no subprocess, no oracle spanning read.
        let live_impl = production
            .split("impl FixtureTransport for LiveTransport")
            .nth(1)
            .expect("live impl");
        let commit_fn = live_impl
            .split("fn commit_reads(&self")
            .nth(1)
            .expect("commit fn")
            .split("\n    fn ")
            .next()
            .unwrap_or("");
        for slow in [
            "socket_get_reply",
            "ax_contains_wid",
            "codesign",
            "read_agreement",
            "designated_tail",
        ] {
            assert!(
                !commit_fn.contains(slow),
                "held-key commit must not call {slow}"
            );
        }
        // The full proof owns the target reads but never the foreground
        // authority (commit reads it last), never the selected attribution
        // (its own step samples it after every blocking read), nor
        // signing.
        let full_fn = live_impl
            .split("fn full_reads(")
            .nth(1)
            .expect("full fn")
            .split("\n    fn ")
            .next()
            .unwrap_or("");
        for slow in [
            "codesign",
            "designated_tail",
            "frontmost_app",
            "read_agreement",
        ] {
            assert!(
                !full_fn.contains(slow),
                "full proof must not fork signing, read foreground, or sample attribution ({slow})"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn owned_child_timeout_kills_and_reaps() {
        // The harness's own non-desktop child: a quick exit collects
        // output, a wedged sleep is killed and reaped past the deadline
        // (no orphan, no zombie - the runner owns the Child on every
        // path) and the timeout reason is explicit.
        let quick = run_child_bounded("echo", &["hello"], Duration::from_secs(5), "test-child")
            .expect("quick child collects");
        assert!(quick.status.success());
        assert_eq!(String::from_utf8_lossy(&quick.stdout).trim(), "hello");
        let start = std::time::Instant::now();
        let err = run_child_bounded("sleep", &["30"], Duration::from_millis(300), "test-child")
            .unwrap_err();
        let elapsed = start.elapsed();
        assert!(err.contains("timed out"), "unexpected: {err}");
        assert!(err.contains("killed and reaped"), "unexpected: {err}");
        assert!(
            elapsed < Duration::from_secs(10),
            "runner returned promptly after the deadline ({elapsed:?})"
        );
        // The runner leaks no children: a second wedged spawn is still
        // bounded the same way.
        let err = run_child_bounded("sleep", &["30"], Duration::from_millis(300), "test-child")
            .unwrap_err();
        assert!(err.contains("killed and reaped"), "unexpected: {err}");
    }

    #[test]
    fn helper_product_and_parked_paths_stay_fixture_proof_free() {
        // The legacy helper owned-post path never consults fixture proof:
        // no shared constructor, no shared session, no relaxed gate.
        // Scope to the production sections so these literals cannot
        // self-match.
        for file in [include_str!("owned_control.rs"), include_str!("synth.rs")] {
            let production = file.split("#[cfg(test)]").next().unwrap_or(file);
            assert!(
                !production.contains("fixture_post")
                    && !production.contains("FixtureSession")
                    && !production.contains("FIXTURE_BUNDLE"),
                "helper path must stay fixture-proof free"
            );
        }
        // The diagnostic route reaches no product path: only the CLI
        // contract, the binary dispatch, and the module itself name it.
        // (Parked-scaffold independence is structural: this module's
        // production code holds no reference to it, asserted above.)
        for file in [
            include_str!("tile.rs"),
            include_str!("input_session.rs"),
            include_str!("workspace_send.rs"),
            include_str!("workspace_select.rs"),
            include_str!("runtime.rs"),
        ] {
            let production = file.split("#[cfg(test)]").next().unwrap_or(file);
            assert!(
                !production.contains("fixture_post"),
                "product path must not reach the diagnostic route"
            );
        }
    }

    #[test]
    fn reports_carry_ids_and_latencies_only() {
        // Schema carries IDs, chord metadata, and dispatch/proof latencies
        // only: no titles, paths, nonces, executables, requirements, or
        // sockets. Caller ids below are synthetic fixture values, never a
        // live record.
        let post = FixturePostReport {
            outcome: "posted".to_owned(),
            pid: 4242,
            wid: 7,
            caller: "a".to_owned(),
            chord: "focus-h".to_owned(),
            action: "focus".to_owned(),
            key_code: 0x04,
            flags: 0x0004_0000,
            down_proof_us: 900,
            down_post_us: 9,
            up_proof_us: 901,
            up_post_us: 11,
            released_bypass: false,
            synthetic: true,
            note: "n".to_owned(),
        };
        let flat = serde_json::to_string(&post).expect("post report flattens");
        for leaked in [
            "title",
            "path",
            "nonce",
            "exe",
            "launch",
            "socket",
            "suffix",
            "signer",
            "requirement",
        ] {
            assert!(!flat.contains(leaked), "schema leaks {leaked}");
        }
        let focus = FixtureFocusReport {
            outcome: "completed".to_owned(),
            pid: 4242,
            wid: 7,
            caller: "a".to_owned(),
            proof_us: 902,
            dispatch: crate::focus::FocusDispatch {
                pid: 4242,
                wid: 7,
                set_front_status: 0,
                post_statuses: vec![0, 0],
                dispatch_latency_ms: 3,
            },
            agreement: crate::focus::FocusAgreement {
                workspace_pid: 4242,
                workspace_bundle: FIXTURE_BUNDLE.to_owned(),
                ax_pid: 4242,
                ax_wid: Some(7),
                cg_wid_owned: true,
                evidence_route: "slps-front-direct-focused".to_owned(),
            },
            focus_latency_ms: 5,
            synthetic: false,
            note: "n".to_owned(),
        };
        let flat = serde_json::to_string(&focus).expect("focus report flattens");
        for leaked in [
            "title",
            "path",
            "nonce",
            "exe",
            "launch",
            "socket",
            "suffix",
            "signer",
            "requirement",
        ] {
            assert!(!flat.contains(leaked), "schema leaks {leaked}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn socket_client_matches_reply_by_request_id() {
        // Real local socket pair with a canned live-shaped server: the
        // client sends one nonce-gated `get` and parses the matched reply.
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        let dir = std::env::temp_dir().join(format!("fxst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test dir creates");
        let path = dir.join("f.sock");
        let reply = socket_reply(4242, "OmniTilerTestWindow:a:run-live");
        let reply_line = serde_json::to_string(&reply).expect("reply encodes");
        let server = UnixListener::bind(&path).expect("test listener binds");
        let path_string = path.to_string_lossy().into_owned();
        let nonce = NONCE.to_owned();
        std::thread::spawn(move || {
            let (mut conn, _) = server.accept().expect("test accept");
            conn.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut reader = BufReader::new(conn.try_clone().expect("test clone"));
            let mut raw = String::new();
            reader.read_line(&mut raw).expect("test read");
            let request: serde_json::Value =
                serde_json::from_str(raw.trim_end()).expect("test request parses");
            assert_eq!(request["op"], "get");
            assert_eq!(request["nonce"], nonce.as_str());
            assert_eq!(request["ids"], serde_json::json!(["a"]));
            // One unrelated line first: the client must ignore it and match
            // by request id under its deadline.
            conn.write_all(b"{\"request_id\":\"other\",\"ok\":true}\n")
                .ok();
            conn.write_all(reply_line.as_bytes()).ok();
            conn.write_all(b"\n").ok();
        });
        let value = socket_get_reply(&path_string, NONCE, "a").expect("client matches the reply");
        let snap = parse_socket_get_reply(&value, SOCKET_REQUEST_ID, "a").expect("parses");
        assert_eq!(snap.pid, 4242);
        assert_eq!(snap.caller, "a");
        assert_eq!(snap.run_tag, "run-live");
        assert_eq!(snap.native_handle, "handle-live-1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn socket_close_and_shutdown_exchange_real_bytes() {
        // Real local socket pair with a nonce-checking double: `close` and
        // `shutdown` carry the exact protocol shapes over real bytes, and a
        // stale nonce is refused by the server side (never trusted).
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        let dir = std::env::temp_dir().join(format!("fxcs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test dir creates");
        let path = dir.join("f.sock");
        let server = UnixListener::bind(&path).expect("test listener binds");
        let path_string = path.to_string_lossy().into_owned();
        let nonce = NONCE.to_owned();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut conn, _) = server.accept().expect("test accept");
                conn.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let mut reader = BufReader::new(conn.try_clone().expect("test clone"));
                let mut raw = String::new();
                reader.read_line(&mut raw).expect("test read");
                let request: serde_json::Value =
                    serde_json::from_str(raw.trim_end()).expect("test request parses");
                assert_eq!(request["v"], 1);
                assert_eq!(request["nonce"], nonce.as_str());
                let op = request["op"].as_str().unwrap_or("").to_owned();
                let request_id = request["request_id"].as_str().unwrap_or("").to_owned();
                let reply = match op.as_str() {
                    "close" => {
                        assert_eq!(request["ids"], serde_json::json!(["a", "b"]));
                        serde_json::json!({
                            "v": 1, "request_id": request_id, "ok": true,
                            "result": {"windows": [
                                {"id": "a", "ok": true, "fields": []},
                                {"id": "b", "ok": true, "fields": []},
                            ]},
                        })
                    }
                    "shutdown" => serde_json::json!({
                        "v": 1, "request_id": request_id, "ok": true, "result": {},
                    }),
                    other => panic!("unexpected op {other}"),
                };
                let mut line = serde_json::to_string(&reply).expect("reply encodes");
                line.push('\n');
                conn.write_all(line.as_bytes()).ok();
            }
        });
        let close = send_fixture_close(&path_string, NONCE, &["a", "b"]).expect("close exchanges");
        assert_eq!(close["ok"], true);
        assert!(send_fixture_close(&path_string, NONCE, &[]).is_err());
        assert!(send_fixture_close(&path_string, NONCE, &["bad id"]).is_err());
        let down = send_fixture_shutdown(&path_string, NONCE).expect("shutdown exchanges");
        assert_eq!(down["ok"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
