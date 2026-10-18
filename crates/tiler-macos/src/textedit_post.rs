#![forbid(unsafe_code)]

//! Diagnostic-only disposable TextEdit key tap (fixtures only, never product).
//!
//! Narrow sibling of [`crate::owned_control::run_owned_post`]: posts ONE
//! benign key down/up (closed vocabulary in [`resolve_textedit_key`], disjoint
//! from the tile chord catalogue) through the session stream to a run-owned
//! disposable TextEdit document. The disposable identity is bound by a
//! run-owned record ([`TextEditRecord`]: exact `(pid, wid)` plus the
//! test-run-created temp directory plus the per-run random token plus a
//! nonce); the helper spawn record is never consulted and helper ownership is
//! unchanged.
//!
//! Before EACH key edge the live boundary re-proves, in order: session gates
//! (SLPS available, AX trusted, no Secure Input, unlocked session, post
//! access), frontmost app exactly `com.apple.TextEdit` at the record PID
//! (Ghostty and every non-TextEdit bundle refuses before any post), the
//! selected-oracle agreement on exactly `(pid, wid)`, the focused/main AX
//! window at exactly `wid` whose document admits disposable proof (file path
//! inside the record temp directory OR title containing the record token),
//! and the CG owner join. Anything mismatched, stale, or unreadable refuses
//! before the window server is touched, and every refusal is logged to
//! stderr as metadata (stages and IDs only).
//!
//! Unconditional release: once the down edge has posted, EVERY exit path
//! posts the key-up through [`raw_session_key_post`] WITHOUT target gates
//! (a drifted target must never strand a held key). The orchestration is
//! [`orchestrate_edges`], unit-tested over injected closures.
//!
//! Diagnostic route only: reachable solely through the `textedit-post` CLI
//! command. No product path references this module (regression-tested), and
//! the posted keys never arm product chords (vocabulary disjointness test).
//! Outputs are metadata-only JSON (IDs, stages, latencies — never titles,
//! content, or paths).

use serde::{Deserialize, Serialize};

/// Exact bundle required frontmost before each edge.
pub const TEXTEDIT_BUNDLE: &str = "com.apple.TextEdit";

/// Per-run token bounds: long enough to be unguessable per run, short enough
/// to stay a title fragment.
pub const MIN_TOKEN_LEN: usize = 8;
pub const MAX_TOKEN_LEN: usize = 64;

/// No modifiers are ever posted on this route (flags are always zero); the
/// release posts the same key up with the same flags.
pub const TEXTEDIT_FLAGS: u64 = 0;

/// Resolve a diagnostic key name to its macOS virtual key code. Closed set of
/// benign text/cancel/navigation keys only — nothing else. Pure;
/// unit-tested. Disjoint from [`crate::owned_control::chord_names`] by
/// construction (bare names, no `focus-`/`move-`/`select-`/`send-` arms).
pub fn resolve_textedit_key(name: &str) -> Option<u16> {
    match name {
        "a" => Some(0x00),
        "return" => Some(0x24),
        "space" => Some(0x31),
        "escape" => Some(0x35),
        "left" => Some(0x7B),
        "right" => Some(0x7C),
        _ => None,
    }
}

/// All diagnostic key names (for help text and the parser test).
#[must_use]
pub fn textedit_key_names() -> &'static [&'static str] {
    &["a", "return", "space", "escape", "left", "right"]
}

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

/// Pure containment: `path` names a file strictly inside `dir` (trailing
/// slashes tolerated on the dir). The `dir + "/"` boundary refuses
/// prefix-sibling escapes (`/tmp/run1-evil/...` is not inside `/tmp/run1`).
/// Never guesses: empty inputs refuse.
#[must_use]
pub(crate) fn path_inside_dir(dir: &str, path: &str) -> bool {
    if dir.is_empty() || path.is_empty() || path.contains('\0') {
        return false;
    }
    let trimmed = dir.trim_end_matches('/');
    if trimmed.is_empty() || !trimmed.starts_with('/') {
        return false;
    }
    let path_bytes = path.as_bytes();
    let dir_bytes = trimmed.as_bytes();
    path_bytes.len() > dir_bytes.len() + 1
        && path_bytes.starts_with(dir_bytes)
        && path_bytes[dir_bytes.len()] == b'/'
}

/// Strip a `file://` URL scheme to a plain path for the containment check.
/// Handles `file:///abs/...` and `file://localhost/abs/...`; any other input
/// passes through unchanged (the containment check then refuses unless it
/// literally matches). Pure; unit-tested.
#[must_use]
pub(crate) fn strip_file_url(raw: &str) -> &str {
    let without_host = raw.strip_prefix("file://localhost").unwrap_or(raw);
    without_host.strip_prefix("file://").unwrap_or(without_host)
}

/// Pure disposable-document verdict over plain data (no OS touch): admits
/// exactly when the document file path is inside the record temp directory
/// OR the window title contains the record token. Unreadable legs (`None`)
/// refuse, never guessed; a missing/empty token or malformed temp dir
/// refuses closed. Unit-tested; the live boundary performs the reads and
/// passes the outcomes here just before its post.
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
    if let Some(raw) = file_path {
        let path = strip_file_url(raw);
        if path_inside_dir(temp_dir, path) {
            return Ok(());
        }
    }
    if let Some(title) = title
        && title.contains(token)
    {
        return Ok(());
    }
    Err("refusing textedit post: disposable identity mismatch (path outside run temp dir and title lacks run token)".to_owned())
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
/// then oracle agreement, then disposable identity — in post order, so the
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

/// Dispatch latencies for one down/up pair (dispatch cost, never delivery).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EdgePairLatency {
    pub down_us: u64,
    pub up_us: u64,
    /// `Some` when the unconditional bypass release posted (up proof or up
    /// post failed after a posted down); `None` on the clean path and when
    /// nothing was held. A failed release attempt is also `None` (the caller
    /// reports `released_bypass: false` and logs loudly).
    pub release_us: Option<u64>,
}

/// Pure edge orchestration with the unconditional-release contract: once
/// `post_down` has succeeded, EVERY exit path runs `release_bypass` (which by
/// contract posts the key-up with no target gates) before returning the
/// error. Nothing held means no release (proof or post-down failure returns
/// with `None`). Unit-tested over injected closures; the live boundary passes
/// live proofs/posts here.
pub(crate) fn orchestrate_edges(
    prove_down: impl Fn() -> Result<(), String>,
    post_down: impl Fn() -> Result<u64, String>,
    prove_up: impl Fn() -> Result<(), String>,
    post_up: impl Fn() -> Result<u64, String>,
    release_bypass: impl Fn() -> Result<u64, String>,
) -> Result<EdgePairLatency, (String, Option<u64>)> {
    prove_down().map_err(|e| (e, None))?;
    let down_us = post_down().map_err(|e| (e, None))?;
    // A key is held from here: every exit releases first.
    match prove_up() {
        Err(e) => Err((e, release_bypass().ok())),
        Ok(()) => match post_up() {
            Ok(up_us) => Ok(EdgePairLatency {
                down_us,
                up_us,
                release_us: None,
            }),
            Err(e) => Err((e, release_bypass().ok())),
        },
    }
}

/// Raw session-stream key post (no gates): session taps observe it exactly
/// like the tile-catalogue posts. Used for BOTH gated edges (after the
/// per-edge proof) and the unconditional bypass release (no proof). Safe
/// `objc2` bindings only; macOS-only (the off-macOS stub refuses).
fn raw_session_key_post(key_code: u16, flags: u64, down: bool) -> Result<u64, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventTapLocation};
        use std::time::Instant;
        let event = CGEvent::new_keyboard_event(None, key_code, down)
            .ok_or_else(|| "CGEventCreateKeyboardEvent returned null".to_owned())?;
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        let start = Instant::now();
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
        Ok(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (key_code, flags, down);
        Err("synthetic key posts are macOS-only".to_owned())
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

#[derive(Debug, Clone, Serialize)]
struct TextEditPostReport {
    outcome: String,
    pid: u32,
    wid: u32,
    key: String,
    key_code: u16,
    down_post_us: u64,
    up_post_us: u64,
    released_bypass: bool,
    synthetic: bool,
    note: String,
}

/// Post ONE diagnostic key down/up to the run-owned disposable TextEdit
/// document bound by the record file. Exit codes: 2 usage/validation, 1
/// gate/proof/post failure with evidence. Every refusal is logged to stderr
/// as metadata (stages and IDs only — never titles, content, or paths).
/// Once the down edge has posted, the up proof or up post failing still
/// releases the key through the gateless bypass before returning.
pub fn run_textedit_post(
    record_path: &str,
    nonce: &str,
    key_name: &str,
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
    let key_code = resolve_textedit_key(key_name)
        .ok_or_else(|| (2, format!("unknown --key '{key_name}' (see help)")))?;
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
    let outcome = orchestrate_edges(
        || prove_edge(&record, "edge-down"),
        || {
            raw_session_key_post(key_code, TEXTEDIT_FLAGS, true)
                .map_err(|e| format!("post-down: {e}"))
        },
        || prove_edge(&record, "edge-up"),
        || {
            raw_session_key_post(key_code, TEXTEDIT_FLAGS, false)
                .map_err(|e| format!("post-up: {e}"))
        },
        || {
            // Unconditional bypass release: no target gates by contract (a
            // drifted target must never strand a held key). Logged loudly.
            eprintln!(
                "textedit-post bypass release: posting key up with no target gates (pid={pid} wid={wid})"
            );
            raw_session_key_post(key_code, TEXTEDIT_FLAGS, false)
        },
    );
    match outcome {
        Ok(latency) => {
            let report = TextEditPostReport {
                outcome: "posted".to_owned(),
                pid,
                wid,
                key: key_name.to_owned(),
                key_code,
                down_post_us: latency.down_us,
                up_post_us: latency.up_us,
                released_bypass: false,
                synthetic: true,
                note: "diagnostic-only synthetic CG session-stream tap (never physical hardware, never helper/product input); delivery is proven harness-side by the disposable document delta, never this dispatch receipt"
                    .to_owned(),
            };
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Err((error, release_us)) => {
            let released_bypass = release_us.is_some();
            if error.contains("edge-up") || error.contains("post-up") {
                eprintln!(
                    "textedit-post up leg failed after posted down; bypass release posted={released_bypass} (pid={pid} wid={wid})"
                );
            }
            let stage = if error.starts_with("edge-down") {
                "oracle-down"
            } else if error.starts_with("edge-up") {
                "oracle-up"
            } else if error.starts_with("post-down") {
                "post-down"
            } else if error.starts_with("post-up") {
                "post-up"
            } else {
                "edge-down"
            };
            Err((
                1,
                serde_json::json!({"stage": stage, "error": error, "released_bypass": released_bypass})
                    .to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_vocabulary_is_closed_and_disjoint_from_tile_chords() {
        assert_eq!(textedit_key_names().len(), 6);
        assert_eq!(resolve_textedit_key("a"), Some(0x00));
        assert_eq!(resolve_textedit_key("return"), Some(0x24));
        assert_eq!(resolve_textedit_key("space"), Some(0x31));
        assert_eq!(resolve_textedit_key("escape"), Some(0x35));
        assert_eq!(resolve_textedit_key("left"), Some(0x7B));
        assert_eq!(resolve_textedit_key("right"), Some(0x7C));
        for name in [
            "", "h", "ctrl-h", "focus-h", "move-h", "float-g", "select-1", "send-1", "A", "SPACE",
            "click", "up", "down", "drag", "tab",
        ] {
            assert!(resolve_textedit_key(name).is_none(), "{name}");
        }
        // Disjoint both ways: diagnostic keys never arm product chords, and
        // no tile chord parses as a diagnostic key (the diagnostic route can
        // never drive product bindings).
        for name in textedit_key_names() {
            assert!(
                crate::owned_control::resolve_chord(name).is_none(),
                "{name}"
            );
        }
        for name in crate::owned_control::chord_names() {
            assert!(resolve_textedit_key(name).is_none(), "{name}");
        }
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

    #[test]
    fn path_containment_refuses_prefix_siblings() {
        assert!(path_inside_dir("/tmp/run1", "/tmp/run1/doc.rtf"));
        assert!(path_inside_dir("/tmp/run1/", "/tmp/run1/sub/doc.rtf"));
        // Prefix sibling, not a child: the boundary slash refuses it.
        assert!(!path_inside_dir("/tmp/run1", "/tmp/run1-evil/doc.rtf"));
        assert!(!path_inside_dir("/tmp/run1", "/tmp/run1"));
        assert!(!path_inside_dir("/tmp/run1", "/tmp/run"));
        assert!(!path_inside_dir("/tmp/run1", ""));
        assert!(!path_inside_dir("", "/tmp/run1/doc.rtf"));
        assert!(!path_inside_dir("/", "/doc.rtf"));
        assert!(!path_inside_dir("/tmp/run1", "/tmp/run1/\0evil"));
    }

    #[test]
    fn file_url_strips_only_file_scheme() {
        assert_eq!(
            strip_file_url("file:///tmp/run1/doc.rtf"),
            "/tmp/run1/doc.rtf"
        );
        assert_eq!(
            strip_file_url("file://localhost/tmp/run1/doc.rtf"),
            "/tmp/run1/doc.rtf"
        );
        assert_eq!(strip_file_url("/tmp/run1/doc.rtf"), "/tmp/run1/doc.rtf");
        assert_eq!(strip_file_url(""), "");
    }

    #[test]
    fn doc_verdict_admits_path_inside_or_title_token() {
        let dir = "/tmp/omnitiler-run-abc123";
        let token = "tok-abc123";
        // Either leg alone admits.
        assert!(
            disposable_doc_verdict(Some("/tmp/omnitiler-run-abc123/doc.rtf"), None, dir, token)
                .is_ok()
        );
        assert!(
            disposable_doc_verdict(
                Some("file:///tmp/omnitiler-run-abc123/doc.rtf"),
                Some("other"),
                dir,
                token
            )
            .is_ok()
        );
        assert!(disposable_doc_verdict(None, Some("notes tok-abc123 draft"), dir, token).is_ok());
        assert!(
            disposable_doc_verdict(
                Some("/elsewhere/doc.rtf"),
                Some("notes tok-abc123 draft"),
                dir,
                token
            )
            .is_ok()
        );
        // Neither leg admits: foreign path plus tokenless title.
        assert!(
            disposable_doc_verdict(Some("/elsewhere/doc.rtf"), Some("notes"), dir, token).is_err()
        );
        // Unreadable legs refuse, never guessed.
        assert!(disposable_doc_verdict(None, None, dir, token).is_err());
        assert!(disposable_doc_verdict(None, Some("notes"), dir, token).is_err());
        // Prefix-sibling path plus tokenless title refuses.
        assert!(
            disposable_doc_verdict(
                Some("/tmp/omnitiler-run-abc123-evil/doc.rtf"),
                Some("notes"),
                dir,
                token
            )
            .is_err()
        );
        // Malformed record shapes refuse closed even with agreeing legs.
        assert!(
            disposable_doc_verdict(
                Some("/tmp/omnitiler-run-abc123/doc.rtf"),
                None,
                "relative",
                token
            )
            .is_err()
        );
        assert!(disposable_doc_verdict(None, Some("notes tok-abc123"), dir, "short").is_err());
        assert!(
            disposable_doc_verdict(Some("/tmp/omnitiler-run-abc123/doc.rtf"), None, dir, "")
                .is_err()
        );
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

    #[test]
    fn edge_verdict_orders_bundle_then_oracle_then_identity() {
        let dir = "/tmp/omnitiler-run-abc123";
        let token = "tok-abc123";
        let good_path = Some("/tmp/omnitiler-run-abc123/doc.rtf");
        assert!(edge_verdict(Some(TEXTEDIT_BUNDLE), true, good_path, None, dir, token).is_ok());
        // Bundle refuses first even when everything else agrees.
        let err = edge_verdict(
            Some("com.mitchellh.ghostty"),
            true,
            good_path,
            None,
            dir,
            token,
        )
        .unwrap_err();
        assert!(err.contains("Ghostty"), "{err}");
        // Oracle refuses before identity is consulted.
        let err =
            edge_verdict(Some(TEXTEDIT_BUNDLE), false, good_path, None, dir, token).unwrap_err();
        assert!(err.contains("oracle"), "{err}");
        // Identity refuses last.
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
    fn orchestration_releases_only_when_a_key_is_held() {
        use std::cell::RefCell;
        // Clean path: no release posted.
        let log: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let out = orchestrate_edges(
            || Ok(()),
            || {
                log.borrow_mut().push("down");
                Ok(3)
            },
            || Ok(()),
            || {
                log.borrow_mut().push("up");
                Ok(5)
            },
            || {
                log.borrow_mut().push("release");
                Ok(7)
            },
        );
        assert_eq!(
            out,
            Ok(EdgePairLatency {
                down_us: 3,
                up_us: 5,
                release_us: None
            })
        );
        assert_eq!(*log.borrow(), vec!["down", "up"]);

        // Down proof fails: nothing held, no release.
        let log: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let out = orchestrate_edges(
            || Err("edge-down: oracle disagrees".to_owned()),
            || {
                log.borrow_mut().push("down");
                Ok(3)
            },
            || Ok(()),
            || {
                log.borrow_mut().push("up");
                Ok(5)
            },
            || {
                log.borrow_mut().push("release");
                Ok(7)
            },
        );
        assert_eq!(out, Err(("edge-down: oracle disagrees".to_owned(), None)));
        assert!(log.borrow().is_empty());

        // Down post fails: nothing held, no release.
        let log: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let out = orchestrate_edges(
            || Ok(()),
            || {
                log.borrow_mut().push("down");
                Err("post-down: boom".to_owned())
            },
            || Ok(()),
            || {
                log.borrow_mut().push("up");
                Ok(5)
            },
            || {
                log.borrow_mut().push("release");
                Ok(7)
            },
        );
        assert_eq!(out, Err(("post-down: boom".to_owned(), None)));
        assert_eq!(*log.borrow(), vec!["down"]);

        // Up proof drifts after a posted down: bypass release posts.
        let log: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let out = orchestrate_edges(
            || Ok(()),
            || {
                log.borrow_mut().push("down");
                Ok(3)
            },
            || Err("edge-up: frontmost pid 9999 != record pid 4242; target drifted".to_owned()),
            || {
                log.borrow_mut().push("up");
                Ok(5)
            },
            || {
                log.borrow_mut().push("release");
                Ok(9)
            },
        );
        assert_eq!(
            out,
            Err((
                "edge-up: frontmost pid 9999 != record pid 4242; target drifted".to_owned(),
                Some(9)
            ))
        );
        assert_eq!(*log.borrow(), vec!["down", "release"]);

        // Up post fails after a posted down: bypass release posts.
        let log: RefCell<Vec<&str>> = RefCell::new(Vec::new());
        let out = orchestrate_edges(
            || Ok(()),
            || {
                log.borrow_mut().push("down");
                Ok(3)
            },
            || Ok(()),
            || {
                log.borrow_mut().push("up");
                Err("post-up: boom".to_owned())
            },
            || {
                log.borrow_mut().push("release");
                Ok(9)
            },
        );
        assert_eq!(out, Err(("post-up: boom".to_owned(), Some(9))));
        assert_eq!(*log.borrow(), vec!["down", "up", "release"]);

        // Failed release attempt is honest: no release latency claimed.
        let out = orchestrate_edges(
            || Ok(()),
            || Ok(3),
            || Err("edge-up: oracle disagrees".to_owned()),
            || Ok(5),
            || Err("macOS-only".to_owned()),
        );
        assert_eq!(out, Err(("edge-up: oracle disagrees".to_owned(), None)));
    }

    #[test]
    fn run_refuses_before_live_touch_without_record() {
        // No record file: the record stage refuses before any session, AX,
        // CG, or post touch (pure file read only). Safe on every platform.
        let err = super::run_textedit_post(
            "/nonexistent-omnitiler-run/record.json",
            "0123456789abcdef0123456789abcdef",
            "a",
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
    fn run_validates_key_and_nonce_before_record() {
        // Unknown keys and empty nonces fail closed with usage code 2 and
        // never reach the record read.
        let err = super::run_textedit_post(
            "/nonexistent-omnitiler-run/record.json",
            "0123456789abcdef0123456789abcdef",
            "focus-h",
        )
        .unwrap_err();
        assert_eq!(err.0, 2, "unexpected: {}", err.1);
        let err = super::run_textedit_post("/nonexistent-omnitiler-run/record.json", "", "a")
            .unwrap_err();
        assert_eq!(err.0, 2, "unexpected: {}", err.1);
    }

    #[test]
    fn helper_ownership_path_stays_textedit_free() {
        // The helper owned-post path never consults disposable TextEdit
        // proof: no relaxation, no shared constructor, no shared record.
        // Scope to the production sections so these literals cannot
        // self-match.
        for file in [include_str!("owned_control.rs"), include_str!("synth.rs")] {
            let production = file.split("#[cfg(test)]").next().unwrap_or(file);
            assert!(
                !production.contains("textedit") && !production.contains("TextEdit-record"),
                "helper path must stay TextEdit-free"
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
