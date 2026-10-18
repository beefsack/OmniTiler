#![forbid(unsafe_code)]

//! Diagnostic-only disposable TextEdit chord tap (fixtures only, never product).
//!
//! Narrow sibling of [`crate::owned_control::run_owned_post`]: posts ONE
//! tile-catalogue chord down/up (the exact [`crate::owned_control`] chord
//! vocabulary, so the product tap test transport - send-2 then send-1 and
//! the selects - can run with TextEdit foreground) through the shared
//! session-stream primitive to a run-owned disposable TextEdit document. The
//! disposable identity is bound by a run-owned record ([`TextEditRecord`]:
//! exact `(pid, wid)` plus the test-run-created temp directory plus the
//! per-run random token plus a nonce); the helper spawn record is never
//! consulted, helper ownership is unchanged, and no previously unarmed
//! product action is armed (the chord set is exactly the owned-post set).
//!
//! Before EACH key edge the live boundary re-proves, in order: session gates
//! (SLPS available, AX trusted, no Secure Input, unlocked session, post
//! access), frontmost app exactly `com.apple.TextEdit` at the record PID
//! (Ghostty and every non-TextEdit bundle refuses before any post), the
//! selected-oracle agreement on exactly `(pid, wid)`, the focused/main AX
//! window at exactly `wid` whose document admits canonical disposable proof
//! (the live document file canonicalizes strictly inside the live run
//! directory - both resolved per edge, so traversal and symlink escapes
//! refuse - OR the title contains the record token), and the CG owner join.
//! Anything mismatched, stale, or unreadable refuses before the window
//! server is touched, and every refusal is logged to stderr as metadata
//! (stages and IDs only).
//!
//! Unconditional cleared-modifier release: once the down edge has posted, an
//! [`EdgeGuard`] owns the pending key. Every return path after a posted down
//! releases explicitly through the guard (gateless key-up with cleared flags,
//! so no modifier can stick), and unwinding through the guard releases via
//! `Drop`. Handled SIGINT/SIGTERM reuse the shared diagnostic interrupt arm:
//! the arm latches delivery and the route checks the latch at explicit
//! sequence points around the posts (no polling loop - the synchronous pair
//! has no hold window - and no timer), releasing first on a latched
//! interrupt. SIGKILL, abort, and power loss cannot release: disclosed
//! residual, same as every existing posting route. The guard mechanics are
//! unit-tested over injected post functions; the armed-`Drop` body and the
//! live signal delivery itself are live-only and reviewed, not unit-driven.
//!
//! Diagnostic route only: reachable solely through the `textedit-post` CLI
//! command. No product path references this module (regression-tested).
//! Outputs are metadata-only JSON (IDs, stages, latencies - never titles,
//! content, or paths).

use serde::{Deserialize, Serialize};

use crate::config::ProbeModifier;

/// Exact bundle required frontmost before each edge.
pub const TEXTEDIT_BUNDLE: &str = "com.apple.TextEdit";

/// Per-run token bounds: long enough to be unguessable per run, short enough
/// to stay a title fragment.
pub const MIN_TOKEN_LEN: usize = 8;
pub const MAX_TOKEN_LEN: usize = 64;

/// Run-owned disposable-document record: exact `(pid, wid)` plus the
/// test-run-created temp directory plus the per-run token plus the run nonce.
/// Written by the run driver next to the disposable document, read back here.
/// `Clone` is for the live boundary only; tests compare records by value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEditRecord {
    pub pid: u32,
    pub wid: u32,
    pub temp_dir: String,
    pub token: String,
    pub nonce: String,
}

/// Pure token shape: per-run random fragment, ASCII word characters plus `-`.
#[must_use]
pub fn token_valid(token: &str) -> bool {
    let len = token.len();
    if !(MIN_TOKEN_LEN..=MAX_TOKEN_LEN).contains(&len) {
        return false;
    }
    token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Pure temp-directory shape: absolute, bounded, NUL-free. Liveness (exists,
/// is a directory) is checked on the live path only.
#[must_use]
pub fn temp_dir_shape_valid(dir: &str) -> bool {
    if dir.is_empty() || dir.len() > 1024 || dir.contains('\0') {
        return false;
    }
    // Disposable documents never live at the filesystem root.
    dir.starts_with('/') && dir.trim_end_matches('/').len() > 1
}

/// Parse an AXDocument value to a plain absolute path. Accepts plain absolute
/// paths, `file:///abs/...`, and `file://localhost/abs/...`. Rejects foreign
/// hosts, hostless non-absolute forms, and percent-encoded forms (nothing is
/// decoded: encoded input refuses conservatively and the title-token leg
/// still gets its chance). Pure; unit-tested.
pub(crate) fn parse_doc_path(raw: &str) -> Result<String, String> {
    if raw.is_empty() || raw.contains('\0') {
        return Err("refusing textedit post: document path empty".to_owned());
    }
    let path = if let Some(after_scheme) = raw.strip_prefix("file://") {
        if let Some(after_host) = after_scheme.strip_prefix("localhost/") {
            if after_host.is_empty() {
                return Err("refusing textedit post: document URL has no path".to_owned());
            }
            format!("/{after_host}")
        } else if after_scheme.starts_with('/') {
            after_scheme.to_owned()
        } else {
            return Err("refusing textedit post: document URL has a foreign host".to_owned());
        }
    } else if raw.starts_with('/') {
        raw.to_owned()
    } else {
        return Err("refusing textedit post: document path not absolute".to_owned());
    };
    if path.contains('%') {
        return Err(
            "refusing textedit post: rejecting percent-encoded document path without decoding"
                .to_owned(),
        );
    }
    Ok(path)
}

/// Canonical containment of the live document in the live run directory:
/// BOTH sides must canonicalize (existing, symlink-resolved) and the
/// document must sit strictly inside the run directory component-wise. This
/// refuses `..` traversal, symlink escapes in either leg, prefix siblings
/// (`/tmp/run1-evil/...` shares no component prefix with `/tmp/run1`), and
/// missing files - never by byte prefix on a raw string. Filesystem reads
/// only, no posts. The caller falls through to the title-token leg on `Err`.
pub(crate) fn canonical_doc_inside_run_dir(run_dir: &str, doc: &str) -> Result<(), String> {
    let run_canon = std::fs::canonicalize(run_dir)
        .map_err(|_| "refusing textedit post: run temp dir not live".to_owned())?;
    if !run_canon.is_dir() {
        return Err("refusing textedit post: run temp dir not a directory".to_owned());
    }
    let doc_canon = std::fs::canonicalize(doc)
        .map_err(|_| "refusing textedit post: document not live".to_owned())?;
    // `Path::starts_with` compares whole components, and equality is refused:
    // the document must be strictly inside, never the directory itself.
    if doc_canon == run_canon || !doc_canon.starts_with(&run_canon) {
        return Err("refusing textedit post: document outside run temp dir".to_owned());
    }
    Ok(())
}

/// Disposable-document verdict over plain inputs plus live canonical reads
/// (no posts): admits exactly when the document file canonicalizes strictly
/// inside the live run directory OR the window title contains the record
/// token. A failed path leg (unparseable URL, non-live file, outside the run
/// directory) falls through to the title leg - the OR contract - and
/// unreadable legs refuse, never guessed. A malformed temp dir or token
/// refuses closed. Live filesystem reads only; unit-tested against real
/// temporary directories; the live boundary passes the AX reads here just
/// before its post.
pub(crate) fn disposable_doc_verdict(
    file_path: Option<&str>,
    title: Option<&str>,
    temp_dir: &str,
    token: &str,
) -> Result<(), String> {
    if !temp_dir_shape_valid(temp_dir) {
        return Err("refusing textedit post: record temp dir malformed".to_owned());
    }
    if !token_valid(token) {
        return Err("refusing textedit post: record token malformed".to_owned());
    }
    if let Some(raw) = file_path
        && let Ok(path) = parse_doc_path(raw)
        && canonical_doc_inside_run_dir(temp_dir, &path).is_ok()
    {
        return Ok(());
    }
    if let Some(title) = title
        && title.contains(token)
    {
        return Ok(());
    }
    Err("refusing textedit post: disposable identity mismatch (document leg refused and title lacks run token)".to_owned())
}

/// Pure frontmost-bundle verdict over plain data (no OS touch): admits
/// exactly [`TEXTEDIT_BUNDLE`]. Ghostty (any case/variant) is named in its
/// refusal; every other non-TextEdit bundle and unknown/empty bundles refuse.
/// Unit-tested; the live boundary reads the frontmost pair and passes it here.
pub(crate) fn front_bundle_verdict(bundle: Option<&str>) -> Result<(), String> {
    match bundle {
        Some(id) if id == TEXTEDIT_BUNDLE => Ok(()),
        Some(id) if crate::scope::is_ghostty_bundle(id) => {
            Err("refusing textedit post to Ghostty (never a target)".to_owned())
        }
        Some(_) => Err("refusing textedit post to non-TextEdit frontmost app".to_owned()),
        None => Err("refusing textedit post with unreadable frontmost app".to_owned()),
    }
}

/// Pure per-edge verdict over plain data (no OS touch): frontmost bundle,
/// then oracle agreement, then disposable identity - in post order, so the
/// first failure names the refusing leg before any post. Unit-tested; the
/// live boundary performs the reads and passes the outcomes here just before
/// each post.
pub(crate) fn edge_verdict(
    front_bundle: Option<&str>,
    oracle_agrees: bool,
    file_path: Option<&str>,
    title: Option<&str>,
    temp_dir: &str,
    token: &str,
) -> Result<(), String> {
    front_bundle_verdict(front_bundle)?;
    if !oracle_agrees {
        return Err("refusing textedit post: selected oracle disagrees".to_owned());
    }
    disposable_doc_verdict(file_path, title, temp_dir, token)
}

/// Pure record authentication over plain data (no OS touch): exact nonce
/// match plus PID/WID floors plus token/temp-dir shapes. The nonce binds the
/// record to this run; a foreign or stale record refuses before any live
/// touch. Unit-tested.
pub(crate) fn verify_record(record: &TextEditRecord, nonce: &str) -> Result<(), String> {
    if nonce.is_empty() || record.nonce.is_empty() {
        return Err("refusing textedit post: empty nonce".to_owned());
    }
    if record.nonce != nonce {
        return Err("refusing textedit post: nonce mismatch (stale or foreign record)".to_owned());
    }
    if record.pid <= 1 {
        return Err(format!(
            "refusing textedit post to reserved pid {}",
            record.pid
        ));
    }
    if record.pid == std::process::id() {
        return Err("refusing textedit post to the controller itself".to_owned());
    }
    if i32::try_from(record.pid).is_err() {
        return Err(format!(
            "refusing textedit post: pid {} out of range",
            record.pid
        ));
    }
    if record.wid == 0 {
        return Err("refusing textedit post to window id 0".to_owned());
    }
    if !temp_dir_shape_valid(&record.temp_dir) {
        return Err("refusing textedit post: record temp dir malformed".to_owned());
    }
    if !token_valid(&record.token) {
        return Err("refusing textedit post: record token malformed".to_owned());
    }
    Ok(())
}

/// Load a run-owned record file. Only reads the exact record path (no
/// directory walk, no fallback); any IO or shape failure refuses before any
/// live touch.
fn load_record(path: &str) -> Result<TextEditRecord, String> {
    if path.is_empty() {
        return Err("textedit-post needs a nonempty --record".to_owned());
    }
    let body = std::fs::read_to_string(path)
        .map_err(|e| format!("refusing textedit post: record unreadable ({e})"))?;
    serde_json::from_str(&body)
        .map_err(|e| format!("refusing textedit post: record malformed ({e})"))
}

/// Pending-key release guard: owns a posted down edge until its up is proven
/// posted. The bypass release posts key-up with cleared flags and no target
/// gates (a drifted target must never strand a held key or a held modifier).
/// Every return path after a posted down releases explicitly through
/// [`EdgeGuard::release_now`]; unwinding through the guard releases via
/// `Drop`. Releases are idempotent: exactly one up is ever posted per guard.
pub(crate) struct EdgeGuard {
    pid: u32,
    key_code: u16,
    down_posted: bool,
    released: bool,
}

impl EdgeGuard {
    pub(crate) fn new(pid: u32, key_code: u16) -> Self {
        Self {
            pid,
            key_code,
            down_posted: false,
            released: false,
        }
    }

    /// Arm after the down edge posted. Before this call the guard is inert
    /// (nothing held, release is a no-op).
    pub(crate) fn note_down_posted(&mut self) {
        self.down_posted = true;
    }

    /// Disarm after the gated up edge posted (nothing held anymore).
    pub(crate) fn note_released(&mut self) {
        self.released = true;
    }

    /// True only while a key is held and unreleased (a release is owed).
    #[must_use]
    pub(crate) fn is_pending(&self) -> bool {
        self.down_posted && !self.released
    }

    /// Gateless cleared-modifier release through an injected post function
    /// (unit seam): key-up with flags cleared to zero, no target gates.
    /// No-op (`None`) unless a release is owed; idempotent. Unit-tested.
    pub(crate) fn release_with(
        &mut self,
        post: impl Fn(u16, u64, bool) -> Result<u64, String>,
    ) -> Option<u64> {
        if !self.is_pending() {
            return None;
        }
        self.released = true;
        post(self.key_code, 0, false).ok()
    }

    /// Live gateless release through the shared session-stream primitive.
    pub(crate) fn release_now(&mut self) -> Option<u64> {
        let (pid, code) = (self.pid, self.key_code);
        self.release_with(move |key_code, flags, down| {
            debug_assert_eq!(key_code, code);
            crate::synth::raw_post_key_event_to_session(pid, key_code, flags, down, None)
                .map(|posted| posted.post_latency_us)
        })
    }
}

impl Drop for EdgeGuard {
    fn drop(&mut self) {
        // Unwind/cancellation backstop: if a panic unwinds past the explicit
        // release paths while a key is held, still let it up gateless.
        if self.is_pending() {
            eprintln!(
                "textedit-post unwind release: posting key up gateless (pid={} wid-held)",
                self.pid
            );
            let _ = self.release_now();
        }
    }
}

/// Shared live session gates before each edge (no posts): SLPS symbols, AX
/// trust, Secure Input, session lock, post access. Pure reads; any failure
/// refuses with its stage name.
fn session_gates() -> Result<(), String> {
    if !crate::ffi::slps_available() {
        return Err("slps-unavailable: SLPS focus symbols unavailable; stop".to_owned());
    }
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err("ax-trust: AX trust not observed true; stop".to_owned());
    }
    // Unknown Secure Input proceeds here exactly like every existing posting
    // gate (`input::secure_input()` maps `None` to `Unknown` and the tap path
    // proceeds on `Inactive | Unknown`; `run_owned_post`, `input-session`,
    // and the tile path all refuse only on `Some(true)`): the symbol always
    // resolves live on macOS, and off-macOS every post refuses as macOS-only
    // downstream, so `None` is unreachable on a live posting path. Named
    // explicitly so unknown is never silently read as inactive.
    if crate::ffi::secure_input_known() == Some(true) {
        return Err("secure-input: Secure Input active; refusing post".to_owned());
    }
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

/// Live disposable-identity legs for one edge: the AX focused/main selection
/// on `pid` must name exactly `wid`, and the window element for `wid` yields
/// its document path plus title. Stale (window gone), mismatched (another
/// window focused), and unreadable legs all refuse; never guessed. The
/// disposable verdict over these legs is drawn by [`edge_verdict`] in
/// [`prove_edge`], never here.
fn read_disposable_legs(pid: u32, wid: u32) -> Result<(Option<String>, Option<String>), String> {
    let pid_i32 = i32::try_from(pid)
        .map_err(|_| format!("refusing textedit post: pid {pid} out of range"))?;
    let direct = crate::ffi::ax_direct_focused_read(pid_i32);
    let selected = direct.selected_wid;
    if selected != Some(wid) {
        return Err(format!(
            "identity: AX focused/main window {selected:?} != record wid {wid} (focused {} main {}); refusing stale target",
            direct.focused_status, direct.main_status
        ));
    }
    let app = crate::ffi::AxElement::application(pid_i32)
        .ok_or_else(|| "identity: AX application element unavailable".to_owned())?;
    let windows = app.windows().map_err(|code| {
        format!(
            "identity: AX window list unreadable ({})",
            crate::ffi::ax_error_name(code)
        )
    })?;
    for window in &windows {
        if crate::ffi::ax_window_id(window) == Some(wid) {
            let path = crate::ffi::ax_string_attribute(window, "AXDocument").map_err(|code| {
                format!(
                    "identity: AXDocument unreadable ({})",
                    crate::ffi::ax_error_name(code)
                )
            })?;
            let title = crate::ffi::ax_window_title(window).map_err(|code| {
                format!(
                    "identity: AXTitle unreadable ({})",
                    crate::ffi::ax_error_name(code)
                )
            })?;
            return Ok((path, title));
        }
    }
    Err(format!(
        "identity: record wid {wid} not among live AX windows; refusing stale target"
    ))
}

/// Live per-edge proof immediately before one post, in post order: session
/// gates, frontmost-TextEdit at the record PID, oracle agreement on exactly
/// `(pid, wid)`, disposable AX identity, CG owner join. Refuses with the
/// leg's stage name before the window server is touched.
fn prove_edge(record: &TextEditRecord, edge: &str) -> Result<(), String> {
    session_gates().map_err(|e| format!("{edge}: {e}"))?;
    let (front_pid, bundle) = crate::ffi::frontmost_app()
        .ok_or_else(|| format!("{edge}: refusing textedit post with unreadable frontmost app"))?;
    let want_pid = i32::try_from(record.pid).map_err(|_| {
        format!(
            "{edge}: refusing textedit post: pid {} out of range",
            record.pid
        )
    })?;
    if front_pid != want_pid {
        return Err(format!(
            "{edge}: refusing textedit post: frontmost pid {front_pid} != record pid {}; target drifted",
            record.pid
        ));
    }
    crate::focus::read_agreement(record.pid, record.wid)
        .map_err(|e| format!("{edge}: oracle disagrees ({e})"))?;
    let (path, title) =
        read_disposable_legs(record.pid, record.wid).map_err(|e| format!("{edge}: {e}"))?;
    // Single ordering authority for bundle-then-oracle-then-identity: the
    // oracle read above already agreed (passed here as agreed), so this
    // judges the fresh frontmost bundle plus the fresh disposable legs.
    edge_verdict(
        Some(bundle.as_str()),
        true,
        path.as_deref(),
        title.as_deref(),
        &record.temp_dir,
        &record.token,
    )
    .map_err(|e| format!("{edge}: {e}"))?;
    crate::cg::verify_wid_owned_by_pid(record.pid, record.wid)
        .map_err(|e| format!("{edge}: CG join disagrees ({e})"))?;
    Ok(())
}

/// Resolve the chord flags exactly like the helper owned-post path: the Ctrl
/// role plus the chord Shift/Alt arms. Shared vocabulary, shared flags.
fn chord_flags(modifier: ProbeModifier, chord: crate::owned_control::SemanticChord) -> u64 {
    let (mut want_flags, _) = crate::input::cg_flag_bits(modifier);
    if chord.shift {
        want_flags |= crate::native_input::CG_SHIFT_BIT;
    }
    if chord.alt {
        want_flags |= crate::native_input::CG_ALT_BIT;
    }
    want_flags
}

#[derive(Debug, Clone, Serialize)]
struct TextEditPostReport {
    outcome: String,
    pid: u32,
    wid: u32,
    chord: String,
    action: String,
    key_code: u32,
    flags: u64,
    down_post_us: u64,
    up_post_us: u64,
    released_bypass: bool,
    synthetic: bool,
    note: String,
}

/// Post ONE tile-catalogue chord down/up to the run-owned disposable TextEdit
/// document bound by the record file. Exit codes: 2 usage/validation, 1
/// gate/proof/post failure with evidence. Every refusal is logged to stderr
/// as metadata (stages and IDs only - never titles, content, or paths).
/// Once the down edge has posted, every return path releases through the
/// guard (gateless cleared-modifier key-up); unwinding through the guard
/// releases via `Drop`. Handled SIGINT/SIGTERM latch the shared diagnostic
/// interrupt arm (armed below before any post) and are checked at explicit
/// sequence points, releasing first; SIGKILL/abort stay undischargeable.
pub fn run_textedit_post(
    record_path: &str,
    nonce: &str,
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
        eprintln!("textedit-post refused at {stage}: {error}");
        err_json(stage, error)
    };
    let chord = crate::owned_control::resolve_chord(chord_name)
        .ok_or_else(|| (2, format!("unknown --chord '{chord_name}' (see help)")))?;
    if nonce.is_empty() {
        return Err((2, "textedit-post needs a nonempty --nonce".to_owned()));
    }
    let record = load_record(record_path).map_err(|e| refuse("record", e))?;
    verify_record(&record, nonce).map_err(|e| refuse("record", e))?;
    if !record.temp_dir.starts_with('/') || !std::path::Path::new(&record.temp_dir).is_dir() {
        return Err(refuse(
            "record",
            "refusing textedit post: record temp dir not a live directory".to_owned(),
        ));
    }
    let pid = record.pid;
    let wid = record.wid;
    let want_flags = chord_flags(modifier, chord);
    let key_code = chord.key_code as u16;
    // Handled-termination arm: SIGINT/SIGTERM latch the shared diagnostic
    // flag from here until return (unregistered on drop, including unwind).
    // Registration failure refuses BEFORE any post. The latch is consulted at
    // explicit sequence points below - never a poll loop - and a latched
    // interrupt releases first like any up-leg failure.
    let _interrupt =
        crate::space_drag_diag_live::InterruptArmed::arm().map_err(|e| refuse("interrupt", e))?;
    let interrupted = || {
        if crate::space_drag_diag_live::diag_interrupted() {
            Err("interrupted: SIGINT/SIGTERM latched; refusing post".to_owned())
        } else {
            Ok(())
        }
    };
    let mut guard = EdgeGuard::new(pid, key_code);
    // Down edge: fresh proof, then the post arms the guard. A latched
    // interrupt before the post refuses with nothing held.
    if let Err(error) = prove_edge(&record, "edge-down") {
        let stage = "oracle-down";
        return Err((
            1,
            serde_json::json!({"stage": stage, "error": error, "released_bypass": false})
                .to_string(),
        ));
    }
    if let Err(error) = interrupted() {
        return Err((
            1,
            serde_json::json!({"stage": "interrupted", "error": error, "released_bypass": false})
                .to_string(),
        ));
    }
    let down_post_us =
        crate::synth::raw_post_key_event_to_session(pid, key_code, want_flags, true, None)
            .map(|posted| posted.post_latency_us)
            .map_err(|e| refuse("post-down", format!("post-down: {e}")))?;
    guard.note_down_posted();
    // A latched interrupt with a key held releases immediately instead of
    // proving the up edge.
    if let Err(error) = interrupted() {
        let release_us = guard.release_now();
        let released_bypass = release_us.is_some();
        eprintln!(
            "textedit-post interrupted after posted down; bypass release posted={released_bypass} (pid={pid} wid={wid})"
        );
        return Err((
            1,
            serde_json::json!({"stage": "interrupted", "error": error, "released_bypass": released_bypass})
                .to_string(),
        ));
    }
    // Up edge: fresh proof, then the gated up disarms the guard. Any failure
    // from here releases explicitly first; unwind releases via Drop. A
    // latched interrupt at either check releases like any up-leg failure.
    if let Err(error) = prove_edge(&record, "edge-up") {
        let release_us = guard.release_now();
        let released_bypass = release_us.is_some();
        eprintln!(
            "textedit-post up proof failed after posted down; bypass release posted={released_bypass} (pid={pid} wid={wid})"
        );
        return Err((
            1,
            serde_json::json!({"stage": "oracle-up", "error": error, "released_bypass": released_bypass})
                .to_string(),
        ));
    }
    if let Err(error) = interrupted() {
        let release_us = guard.release_now();
        let released_bypass = release_us.is_some();
        eprintln!(
            "textedit-post interrupted after posted down; bypass release posted={released_bypass} (pid={pid} wid={wid})"
        );
        return Err((
            1,
            serde_json::json!({"stage": "interrupted", "error": error, "released_bypass": released_bypass})
                .to_string(),
        ));
    }
    match crate::synth::raw_post_key_event_to_session(pid, key_code, want_flags, false, None) {
        Ok(posted) => {
            guard.note_released();
            let report = TextEditPostReport {
                outcome: "posted".to_owned(),
                pid,
                wid,
                chord: chord_name.to_owned(),
                action: chord.action.to_owned(),
                key_code: chord.key_code,
                flags: want_flags,
                down_post_us,
                up_post_us: posted.post_latency_us,
                released_bypass: false,
                synthetic: true,
                note: "diagnostic-only synthetic CG session-stream tap of a tile-catalogue chord to the run-owned disposable TextEdit document (never physical hardware, never helper input); delivery is proven harness-side by the tap counters plus the disposable document delta, never this dispatch receipt"
                    .to_owned(),
            };
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Err(e) => {
            let release_us = guard.release_now();
            let released_bypass = release_us.is_some();
            eprintln!(
                "textedit-post up post failed after posted down; bypass release posted={released_bypass} (pid={pid} wid={wid})"
            );
            Err((
                1,
                serde_json::json!({"stage": "post-up", "error": format!("post-up: {e}"), "released_bypass": released_bypass})
                    .to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_interface_reuses_exactly_the_owned_post_vocabulary() {
        // The diagnostic route drives the product tap test transport
        // (send-2 then send-1 and the selects), so it resolves through the
        // single owned-control chord authority - at minimum send-1..9 and
        // select-1..9, in practice the whole bounded owned-post set.
        for name in crate::owned_control::chord_names() {
            assert!(
                crate::owned_control::resolve_chord(name).is_some(),
                "{name}"
            );
        }
        for name in ["send-1", "send-2", "select-1", "select-2"] {
            let chord = crate::owned_control::resolve_chord(name).unwrap();
            assert_eq!(
                chord.action,
                if name.starts_with("send") {
                    "send"
                } else {
                    "select"
                }
            );
        }
        // Nothing previously unarmed is armed here: trailing select-0/send-0,
        // the excluded Up/Down arms, and unknown names refuse at the same
        // authority the helper path uses.
        for name in [
            "select-0",
            "send-0",
            "focus-up",
            "focus-down",
            "move-up",
            "workspace-select-1",
            "a",
            "space",
            "escape",
            "",
        ] {
            assert!(
                crate::owned_control::resolve_chord(name).is_none(),
                "{name}"
            );
        }
    }

    #[test]
    fn chord_flags_match_the_helper_owned_post_composition() {
        // Same composition as run_owned_post: Ctrl role plus chord arms, so a
        // send chord posts exactly the product Ctrl+Shift+digit flags.
        use crate::native_input::{CG_ALT_BIT, CG_SHIFT_BIT};
        const CTRL: u64 = 0x0004_0000;
        let send = crate::owned_control::resolve_chord("send-2").unwrap();
        assert!(send.shift && !send.alt);
        assert_eq!(chord_flags(ProbeModifier::Ctrl, send), CTRL | CG_SHIFT_BIT);
        let select = crate::owned_control::resolve_chord("select-2").unwrap();
        assert!(!select.shift && !select.alt);
        let (base, _) = crate::input::cg_flag_bits(ProbeModifier::Ctrl);
        assert_eq!(chord_flags(ProbeModifier::Ctrl, select), base);
        let shrink = crate::owned_control::resolve_chord("shrink-h").unwrap();
        assert_eq!(
            chord_flags(ProbeModifier::Ctrl, shrink),
            base | CG_SHIFT_BIT | CG_ALT_BIT
        );
    }

    #[test]
    fn token_bounds_admit_only_per_run_fragments() {
        assert!(token_valid("abcdefgh"));
        assert!(token_valid("a1-_-b2c3"));
        assert!(token_valid(&"x".repeat(64)));
        assert!(!token_valid(""));
        assert!(!token_valid("short"));
        assert!(!token_valid(&"x".repeat(65)));
        assert!(!token_valid("has space1"));
        assert!(!token_valid("has/slash1"));
        assert!(!token_valid("has.dot01"));
        assert!(!token_valid("tok-\u{00e9}-1234"));
    }

    #[test]
    fn temp_dir_shape_admits_only_absolute_narrow_dirs() {
        assert!(temp_dir_shape_valid("/tmp/run-abc123"));
        assert!(temp_dir_shape_valid("/tmp/run-abc123/"));
        assert!(!temp_dir_shape_valid(""));
        assert!(!temp_dir_shape_valid("relative/dir"));
        assert!(!temp_dir_shape_valid("/"));
        assert!(!temp_dir_shape_valid("//"));
        assert!(!temp_dir_shape_valid("/tmp/\0evil"));
    }

    /// Real-filesystem fixture: a unique run directory with writer plus
    /// best-effort cleanup. Unix-only: the macOS proof contract is
    /// POSIX-path shaped (`temp_dir_shape_valid` requires a leading `/`),
    /// so live-filesystem admission fixtures are only meaningful where the
    /// platform temp dir is POSIX. No timers, no randomness beyond the PID
    /// plus a process-local counter.
    #[cfg(unix)]
    struct TempRun {
        dir: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl TempRun {
        fn create(tag: &str) -> Self {
            static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "omnitiler-textedit-proof-{}-{tag}-{n}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn write(&self, rel: &str, body: &str) -> std::path::PathBuf {
            let path = self.dir.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, body).unwrap();
            path
        }

        fn as_str(&self) -> &str {
            self.dir.to_str().unwrap()
        }
    }

    #[cfg(unix)]
    impl Drop for TempRun {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn doc_url_parsing_accepts_only_local_absolute_unencoded() {
        assert_eq!(
            parse_doc_path("/tmp/run1/doc.rtf").unwrap(),
            "/tmp/run1/doc.rtf"
        );
        assert_eq!(
            parse_doc_path("file:///tmp/run1/doc.rtf").unwrap(),
            "/tmp/run1/doc.rtf"
        );
        assert_eq!(
            parse_doc_path("file://localhost/tmp/run1/doc.rtf").unwrap(),
            "/tmp/run1/doc.rtf"
        );
        // Foreign hosts, hostless non-absolute forms, encoded forms, and
        // empty/NUL inputs all refuse (the title leg still gets its chance).
        for raw in [
            "",
            "relative/doc.rtf",
            "file://evilhost/tmp/doc.rtf",
            "file://localhost",
            "file://localhost/",
            "FILE:///tmp/doc.rtf",
            "/tmp/run%20space/doc.rtf",
            "file:///tmp/run%2F/doc.rtf",
            "/tmp/\0evil",
        ] {
            assert!(parse_doc_path(raw).is_err(), "{raw}");
        }
    }

    /// Live-filesystem containment: unix-only (see `TempRun`). The portable
    /// pure-shape and title-token legs are covered by
    /// `doc_verdict_title_token_or_without_live_path` on every platform.
    #[cfg(unix)]
    #[test]
    fn canonical_containment_needs_live_strictly_inside_files() {
        let run = TempRun::create("inside");
        let doc = run.write("doc.rtf", "body");
        let dir = run.as_str().to_owned();
        let doc_str = doc.to_str().unwrap().to_owned();
        // Live file strictly inside the live run dir admits.
        assert!(canonical_doc_inside_run_dir(&dir, &doc_str).is_ok());
        // A nested file admits too.
        let nested = run.write("sub/nested.rtf", "n");
        assert!(canonical_doc_inside_run_dir(&dir, nested.to_str().unwrap()).is_ok());
        // The run directory itself is not strictly inside itself.
        assert!(canonical_doc_inside_run_dir(&dir, &dir).is_err());
        // Missing document refuses (non-live leg, never guessed).
        assert!(canonical_doc_inside_run_dir(&dir, &format!("{dir}/missing.rtf")).is_err());
        // Missing run dir refuses; a regular file as the run dir refuses.
        assert!(canonical_doc_inside_run_dir("/nonexistent-omnitiler-run-xyz", &doc_str).is_err());
        assert!(canonical_doc_inside_run_dir(&doc_str, &doc_str).is_err());
        // `..` traversal to a live outside file refuses after normalization
        // (a byte prefix would have admitted this exact string).
        let outside = TempRun::create("outside");
        let secret = outside.write("secret.rtf", "s");
        let outside_name = outside
            .dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        run.write("sub/keep", "k");
        let traversal = format!("{dir}/sub/../../{outside_name}/secret.rtf");
        assert_eq!(
            std::fs::canonicalize(&traversal).unwrap(),
            std::fs::canonicalize(&secret).unwrap()
        );
        assert!(canonical_doc_inside_run_dir(&dir, &traversal).is_err());
        // Prefix sibling with a live file refuses component-wise.
        let sibling_dir = std::path::PathBuf::from(format!("{dir}-evil"));
        let _ = std::fs::remove_dir_all(&sibling_dir);
        std::fs::create_dir_all(&sibling_dir).unwrap();
        let sibling_doc = sibling_dir.join("doc.rtf");
        std::fs::write(&sibling_doc, "e").unwrap();
        assert!(canonical_doc_inside_run_dir(&dir, sibling_doc.to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&sibling_dir);
    }

    #[cfg(unix)]
    #[test]
    fn canonical_containment_refuses_symlink_escape() {
        let run = TempRun::create("linkrun");
        let outside = TempRun::create("linkout");
        let secret = outside.write("secret.rtf", "s");
        // The link path byte-sits inside the run dir, but its target lives
        // outside: canonical comparison refuses.
        let escape = run.dir.join("escape.rtf");
        std::os::unix::fs::symlink(&secret, &escape).unwrap();
        assert!(canonical_doc_inside_run_dir(run.as_str(), escape.to_str().unwrap()).is_err());
        // The same escape refuses through the verdict with a tokenless title.
        assert!(
            disposable_doc_verdict(
                Some(escape.to_str().unwrap()),
                Some("notes"),
                run.as_str(),
                "tok-abc123",
            )
            .is_err()
        );
        // A link staying inside still admits.
        let inner = run.write("inner.rtf", "i");
        let alias = run.dir.join("alias.rtf");
        std::os::unix::fs::symlink(&inner, &alias).unwrap();
        assert!(canonical_doc_inside_run_dir(run.as_str(), alias.to_str().unwrap()).is_ok());
    }

    /// Portable record-shape contract: the macOS proof requires POSIX-shaped
    /// temp dirs (leading `/`), so Windows-style paths refuse at shape
    /// validation on every platform - before any filesystem touch. This pins
    /// why live-filesystem admission fixtures are unix-gated: production
    /// `temp_dir_shape_valid` is deliberately not widened for them.
    #[test]
    fn temp_dir_shape_rejects_windows_paths_on_every_platform() {
        for dir in [
            "C:\\Users\\run\\temp",
            "C:/Users/run/temp",
            "\\\\server\\share\\run",
            "relative\\windows\\dir",
        ] {
            assert!(!temp_dir_shape_valid(dir), "{dir}");
        }
        // POSIX shapes still admit (liveness is proven separately per edge).
        assert!(temp_dir_shape_valid("/tmp/omnitiler-run-abc123"));
    }

    /// Portable title-token OR: with non-live paths (missing on every
    /// platform) the path leg always fails and the title leg alone decides.
    /// Live canonical admission is covered unix-only by
    /// `doc_verdict_canonical_path_admits_live_inside_file`.
    #[test]
    fn doc_verdict_title_token_or_without_live_path() {
        let dir = "/tmp/omnitiler-run-abc123";
        let token = "tok-abc123";
        // Token title admits with a foreign or missing path (title leg).
        assert!(
            disposable_doc_verdict(
                Some("/elsewhere/doc.rtf"),
                Some("notes tok-abc123 draft"),
                dir,
                token
            )
            .is_ok()
        );
        assert!(disposable_doc_verdict(None, Some("notes tok-abc123 draft"), dir, token).is_ok());
        // Foreign or missing path plus tokenless title refuses.
        assert!(
            disposable_doc_verdict(Some("/elsewhere/doc.rtf"), Some("notes"), dir, token).is_err()
        );
        assert!(
            disposable_doc_verdict(
                Some("/tmp/omnitiler-run-abc123/missing.rtf"),
                None,
                dir,
                token
            )
            .is_err()
        );
        // Unreadable legs refuse, never guessed.
        assert!(disposable_doc_verdict(None, None, dir, token).is_err());
        assert!(disposable_doc_verdict(None, Some("notes"), dir, token).is_err());
        // Malformed record shapes refuse closed even with agreeing legs.
        assert!(
            disposable_doc_verdict(Some("/elsewhere/doc.rtf"), None, "relative", token).is_err()
        );
        assert!(disposable_doc_verdict(None, Some("notes tok-abc123"), dir, "short").is_err());
        assert!(disposable_doc_verdict(Some("/elsewhere/doc.rtf"), None, dir, "").is_err());
    }

    /// Live canonical admission through the verdict: unix-only (see
    /// `TempRun`). A live inside file admits with a tokenless title, and a
    /// live prefix-sibling file with a tokenless title refuses.
    #[cfg(unix)]
    #[test]
    fn doc_verdict_canonical_path_admits_live_inside_file() {
        let run = TempRun::create("verdict");
        let token = "tok-abc123";
        let dir = run.as_str().to_owned();
        let doc = run.write("doc.rtf", "body");
        let doc_str = doc.to_str().unwrap().to_owned();
        let file_url = format!("file://localhost{doc_str}");
        assert!(disposable_doc_verdict(Some(&doc_str), Some("notes"), &dir, token).is_ok());
        assert!(disposable_doc_verdict(Some(&file_url), None, &dir, token).is_ok());
        let sibling_dir = std::path::PathBuf::from(format!("{dir}-evil"));
        let _ = std::fs::remove_dir_all(&sibling_dir);
        std::fs::create_dir_all(&sibling_dir).unwrap();
        let sibling_doc = sibling_dir.join("doc.rtf");
        std::fs::write(&sibling_doc, "e").unwrap();
        assert!(
            disposable_doc_verdict(
                Some(sibling_doc.to_str().unwrap()),
                Some("notes"),
                &dir,
                token
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&sibling_dir);
    }

    #[test]
    fn front_bundle_verdict_names_ghostty_and_refuses_non_textedit() {
        assert!(front_bundle_verdict(Some(TEXTEDIT_BUNDLE)).is_ok());
        // Ghostty in any case/variant is named in its refusal.
        for bundle in [
            "com.mitchellh.ghostty",
            "COM.MITCHELLH.GHOSTTY",
            "com.mitchellh.ghostty-beta",
        ] {
            let err = front_bundle_verdict(Some(bundle)).unwrap_err();
            assert!(err.contains("Ghostty"), "{bundle}: {err}");
        }
        // Every other bundle, empty, and unreadable refuse.
        for bundle in [
            "com.apple.Terminal",
            "com.apple.Finder",
            "",
            "com.apple.textedit",
        ] {
            assert!(front_bundle_verdict(Some(bundle)).is_err(), "{bundle}");
        }
        assert!(front_bundle_verdict(None).is_err());
    }

    /// Portable edge ordering: the admitting leg uses a token title (no live
    /// filesystem on any platform); bundle, oracle, and identity refusals
    /// keep their post order. Live canonical admission through the edge is
    /// covered unix-only by `doc_verdict_canonical_path_admits_live_inside_file`.
    #[test]
    fn edge_verdict_orders_bundle_then_oracle_then_identity() {
        let dir = "/tmp/omnitiler-run-abc123";
        let token = "tok-abc123";
        let good_title = Some("notes tok-abc123 draft");
        assert!(edge_verdict(Some(TEXTEDIT_BUNDLE), true, None, good_title, dir, token).is_ok());
        // Bundle refuses first even when everything else agrees.
        let err = edge_verdict(
            Some("com.mitchellh.ghostty"),
            true,
            None,
            good_title,
            dir,
            token,
        )
        .unwrap_err();
        assert!(err.contains("Ghostty"), "{err}");
        // Oracle refuses before identity is consulted.
        let err =
            edge_verdict(Some(TEXTEDIT_BUNDLE), false, None, good_title, dir, token).unwrap_err();
        assert!(err.contains("oracle"), "{err}");
        // Identity refuses last (missing path plus tokenless title).
        let err = edge_verdict(
            Some(TEXTEDIT_BUNDLE),
            true,
            Some("/elsewhere/doc.rtf"),
            Some("notes"),
            dir,
            token,
        )
        .unwrap_err();
        assert!(err.contains("disposable identity"), "{err}");
    }

    fn test_record() -> TextEditRecord {
        TextEditRecord {
            pid: 4242,
            wid: 7,
            temp_dir: "/tmp/omnitiler-run-abc123".to_owned(),
            token: "tok-abc123".to_owned(),
            nonce: "0123456789abcdef0123456789abcdef".to_owned(),
        }
    }

    #[test]
    fn record_verify_binds_nonce_and_floors() {
        let record = test_record();
        assert!(verify_record(&record, "0123456789abcdef0123456789abcdef").is_ok());
        // Nonce mismatch (stale/foreign run) refuses.
        assert!(verify_record(&record, "ffffffffffffffffffffffffffffffff").is_err());
        assert!(verify_record(&record, "").is_err());
        let mut empty_nonce = record.clone();
        empty_nonce.nonce = String::new();
        assert!(verify_record(&empty_nonce, "0123456789abcdef0123456789abcdef").is_err());
        // Reserved/self/out-of-range PIDs and wid 0 refuse.
        for pid in [0, 1, std::process::id()] {
            let mut r = record.clone();
            r.pid = pid;
            assert!(verify_record(&r, &record.nonce).is_err(), "pid={pid}");
        }
        assert!(
            verify_record(
                &TextEditRecord {
                    pid: u32::MAX,
                    ..record.clone()
                },
                &record.nonce
            )
            .is_err()
        );
        assert!(
            verify_record(
                &TextEditRecord {
                    wid: 0,
                    ..record.clone()
                },
                &record.nonce
            )
            .is_err()
        );
        // Malformed token/temp-dir refuse.
        assert!(
            verify_record(
                &TextEditRecord {
                    token: "short".to_owned(),
                    ..record.clone()
                },
                &record.nonce
            )
            .is_err()
        );
        assert!(
            verify_record(
                &TextEditRecord {
                    temp_dir: "relative".to_owned(),
                    ..record.clone()
                },
                &record.nonce
            )
            .is_err()
        );
        // Record round-trips through JSON (the run driver writes this shape).
        let body = serde_json::to_string(&record).unwrap();
        let back: TextEditRecord = serde_json::from_str(&body).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn guard_releases_only_while_a_key_is_held() {
        use std::cell::RefCell;
        let log: RefCell<Vec<(u16, u64, bool)>> = RefCell::new(Vec::new());
        let fake = |code: u16, flags: u64, down: bool| {
            log.borrow_mut().push((code, flags, down));
            Ok(11u64)
        };
        // Fresh guard is inert: release is a no-op and posts nothing.
        let mut guard = EdgeGuard::new(4242, 0x12);
        assert!(!guard.is_pending());
        assert_eq!(guard.release_with(fake), None);
        assert!(log.borrow().is_empty());
        // Armed guard releases exactly once with cleared flags, then is inert.
        guard.note_down_posted();
        assert!(guard.is_pending());
        assert_eq!(guard.release_with(fake), Some(11));
        assert!(!guard.is_pending());
        assert_eq!(*log.borrow(), vec![(0x12, 0, false)]);
        assert_eq!(guard.release_with(fake), None);
        assert_eq!(log.borrow().len(), 1);
        // Gated up disarms: the later drop posts nothing.
        let mut guard = EdgeGuard::new(4242, 0x12);
        guard.note_down_posted();
        guard.note_released();
        assert!(!guard.is_pending());
        assert_eq!(guard.release_with(fake), None);
        assert!(log.borrow().len() == 1);
    }

    #[test]
    fn guard_bypass_clears_modifiers_even_for_shifted_chords() {
        // A send chord posts with Ctrl+Shift held; its bypass release still
        // posts key-up with flags cleared to zero so no modifier can stick.
        use std::cell::RefCell;
        let log: RefCell<Vec<(u16, u64, bool)>> = RefCell::new(Vec::new());
        let mut guard = EdgeGuard::new(4242, 0x12);
        guard.note_down_posted();
        let out = guard.release_with(|code, flags, down| {
            log.borrow_mut().push((code, flags, down));
            Ok(9u64)
        });
        assert_eq!(out, Some(9));
        assert_eq!(*log.borrow(), vec![(0x12, 0, false)]);
    }

    #[test]
    fn disarmed_guard_drops_without_posting() {
        // The Drop backstop only fires while pending: a disarmed guard drops
        // with no live touch on any platform (the armed-Drop body is
        // live-only and reviewed, not unit-driven).
        let mut guard = EdgeGuard::new(4242, 0x12);
        guard.note_down_posted();
        guard.note_released();
        drop(guard);
    }

    #[test]
    fn run_refuses_before_live_touch_without_record() {
        // No record file: the record stage refuses before any session, AX,
        // CG, or post touch (pure file read only). Safe on every platform.
        let err = super::run_textedit_post(
            "/nonexistent-omnitiler-run/record.json",
            "0123456789abcdef0123456789abcdef",
            "send-2",
            ProbeModifier::Ctrl,
        )
        .unwrap_err();
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("\"stage\":\"record\""),
            "unexpected: {}",
            err.1
        );
    }

    #[test]
    fn run_validates_chord_and_nonce_before_record() {
        // Unarmed chords and empty nonces fail closed with usage code 2 and
        // never reach the record read.
        let err = super::run_textedit_post(
            "/nonexistent-omnitiler-run/record.json",
            "0123456789abcdef0123456789abcdef",
            "select-0",
            ProbeModifier::Ctrl,
        )
        .unwrap_err();
        assert_eq!(err.0, 2, "unexpected: {}", err.1);
        let err = super::run_textedit_post(
            "/nonexistent-omnitiler-run/record.json",
            "",
            "send-2",
            ProbeModifier::Ctrl,
        )
        .unwrap_err();
        assert_eq!(err.0, 2, "unexpected: {}", err.1);
    }

    #[test]
    fn helper_ownership_path_stays_disposable_proof_free() {
        // The helper owned-post path never consults disposable-document
        // proof: no shared constructor, no shared record, no relaxed gate.
        // Scope to the production sections so these literals cannot
        // self-match.
        for file in [include_str!("owned_control.rs"), include_str!("synth.rs")] {
            let production = file.split("#[cfg(test)]").next().unwrap_or(file);
            assert!(
                !production.contains("textedit")
                    && !production.contains("TextEdit-record")
                    && !production.contains("TextEditRecord")
                    && !production.contains("EdgeGuard"),
                "helper path must stay disposable-proof free"
            );
        }
        // The diagnostic route reaches no product path: only the CLI
        // contract, the binary dispatch, and the module itself name it.
        for file in [
            include_str!("tile.rs"),
            include_str!("input_session.rs"),
            include_str!("workspace_send.rs"),
            include_str!("runtime.rs"),
        ] {
            let production = file.split("#[cfg(test)]").next().unwrap_or(file);
            assert!(
                !production.contains("textedit_post"),
                "product path must not reach the diagnostic route"
            );
        }
    }
}
