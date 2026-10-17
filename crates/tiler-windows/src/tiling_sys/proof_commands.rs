#![forbid(unsafe_code)]
use super::*;

/// `tile-proof` command: owned-helpers-only proof loop. Requires a nonempty
/// valid `--allowlist` at parse and at native start; malformed, empty,
/// duplicate, untagged, or non-owned entries refuse before any lease or write
/// and never fall back to normal mode. The raw received argv travels with the
/// parsed options into the proof-only audit (never production logs) with a
/// parsed/raw consistency check, so flag delivery is evidenced. Every setter
/// carries a proof audit record (requested/native target plus flags/outcome);
/// production logs stay token-only. Proof installs no keyboard hook; automated
/// synthetic-input verification uses the separate `shortcut-proof` command.
pub fn cmd_tile_proof(options: &TileProofOptions, raw_argv: &[String]) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig::disabled();
    // Proof installs no keyboard hook and takes no Snap setting: the proof
    // contract is unchanged unless a future explicit opt-in lands.
    crate::lifecycle::sys::run_product(trace, false, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: false,
                workspace_proof: false,
                raw_argv,
                keyboard,
                mouse_snap: false,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `shortcut-proof` command: owned-helpers-only automated shortcut proof.
/// Same frozen-allowlist geometry gate as `tile-proof` (never falls back to
/// normal), but the owner installs the hook with test-only acceptance of
/// exactly [`crate::snapkey::SHORTCUT_PROOF_MARKER`] in `dwExtraInfo` and
/// drives the same keyboard dispatcher (fresh observation per intent, Engine
/// focus/move, native readback) plus the same session-only mouse routines.
/// Unshifted Win+L stays gated off (no flag offers it): live runs must never
/// send Win+L. Raw argv travels into the proof-only audit with a consistency
/// check; production logs stay token-only.
pub fn cmd_shortcut_proof(
    options: &crate::tiling::ShortcutProofOptions,
    raw_argv: &[String],
) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_shortcut_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let mouse_snap = !options.no_mouse_snap_prevention;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: true,
                workspace_proof: false,
                raw_argv,
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `workspace-proof` command: owned-helpers-only automated workspace proof.
/// Same frozen-allowlist geometry gate as `shortcut-proof` plus the workspace
/// dispatcher (select/send/follow with hide/reveal) for exactly the
/// allowlisted helpers. The hook accepts exactly
/// [`crate::snapkey::SHORTCUT_PROOF_MARKER`]; every workspace hide verifies
/// `verify_proof_owned` fresh before the write. `shortcut-proof` never hides
/// for workspace intents and `tile-proof` installs no hook.
pub fn cmd_workspace_proof(
    options: &crate::tiling::WorkspaceProofOptions,
    raw_argv: &[String],
) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_workspace_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let mouse_snap = !options.no_mouse_snap_prevention;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: false,
                workspace_proof: true,
                raw_argv,
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `hide-proof` command: owned-helpers-only visibility proof over the product
/// nonce mechanism. Requires a nonempty valid `--allowlist`; every frozen
/// entry is verified as an owned helper (sibling exe/class/lifetime-tag via
/// [`verify_proof_owned`]) BEFORE the ordinary product nonce APIs run, so
/// helper-only gates are never weakened. Each verified helper is admitted with a fresh product nonce and hidden with a
/// write-before-hide ledger commit (schema v4) under the central product
/// watcher; graceful stop auto-reveals via the guarded teardown, and
/// emergency loss auto-reveals via the watcher with idempotent independent
/// restore. Installs no hook, takes no Snap setting, moves no geometry.
pub fn cmd_hide_proof(options: &HideProofOptions, raw_argv: &[String]) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_hide_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let raw_argv = raw_argv.to_vec();
    // No Snap prevention, no hook, no geometry: visibility proof only.
    crate::lifecycle::sys::run_product(trace, false, move |dir, me, store| {
        run_hide_proof_loop(dir, me, store, entries, seconds, trace, raw_argv)
    })
}

fn run_hide_proof_loop(
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    entries: Vec<AllowEntry>,
    seconds: Option<u64>,
    trace: bool,
    raw_argv: Vec<String>,
) -> Result<()> {
    ensure_pm_v2()?;
    let log_path = log_path_for(dir, &me.process_creation);
    let audit_path = dir.join(format!("proof-audit-{}.jsonl", me.process_creation));
    if audit_path.exists() {
        return Err(err("refuse: audit preexists, will not overwrite"));
    }
    let digest = allowlist_digest(&entries);
    let count = entries.len();
    log_json_at(
        &audit_path,
        serde_json::json!({
            "event": "proof-start",
            "argv": raw_argv,
            "seconds": seconds,
            "trace": trace,
            "mode": "hide-proof",
            "allowlist_digest": digest,
            "allowlist_count": count,
        }),
    );
    // Frozen-helper gate BEFORE any product nonce API: every entry must verify
    // as an owned helper, never an ordinary window. Opaque index only in
    // product errors; raw identity stays in the proof audit below.
    for (index, entry) in entries.iter().enumerate() {
        verify_proof_owned(entry.hwnd, entry, me).map_err(|reason| {
            err(format!(
                "refuse: hide-proof allowlist non-owned index={index} {reason}"
            ))
        })?;
    }
    let frozen: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "hwnd": e.hwnd,
                "pid": e.pid,
                "process_creation": e.process_creation,
                "exe_path": e.exe_path,
                "user_sid": e.user_sid,
                "session_id": e.session_id,
                "tag": e.tag,
            })
        })
        .collect();
    log_json_at(
        &audit_path,
        serde_json::json!({"event": "proof-frozen", "windows": frozen}),
    );
    let areas = all_monitors()?;
    let monitor = areas[0].clone();
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "hide-proof-start",
            "mode": "hide-proof",
            "trace": trace,
            "monitors": areas.len(),
            "work": [monitor.work.x, monitor.work.y, monitor.work.w, monitor.work.h],
            "full": [monitor.full.x, monitor.full.y, monitor.full.w, monitor.full.h],
            "allowlist_digest": digest,
            "allowlist_count": count,
        }),
    );
    // Admit + hide each verified helper via the ordinary product APIs.
    // Write-before-hide holds per window: the ledger commit precedes the
    // hide. Opaque index only in product errors; raw identity stays in the
    // proof audit below.
    let mut hidden = 0usize;
    let mut admitted: Vec<crate::model::WindowIdentity> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let claim = crate::product_hide::sys::admit_product_target(entry.hwnd, me)
            .map_err(|e| err(format!("error: hide-proof admit index={index} {e}")))?;
        crate::product_hide::sys::hide_committed_product(store, me, &claim, dir)
            .map_err(|e| err(format!("error: hide-proof hide index={index} {e}")))?;
        admitted.push(claim);
        hidden += 1;
        log_json_at(
            &log_path,
            serde_json::json!({"event": "hide-proof-hide", "index": hidden, "count": count}),
        );
        log_json_at(
            &audit_path,
            serde_json::json!({
                "event": "hide-proof-hide",
                "hwnd": entry.hwnd,
                "pid": entry.pid,
                "process_creation": entry.process_creation,
                "tag": entry.tag,
            }),
        );
    }
    // Ledger receipt check: current version with exactly the admitted product
    // claims, verified field-by-field (kind, HWND, full process, tag), not by
    // count alone, so a substituted claim cannot pass the receipt.
    match store.committed() {
        Ok(Some(record)) => {
            let exact = record.v == crate::model::LEDGER_SCHEMA_VERSION
                && record.windows.len() == admitted.len()
                && admitted.iter().all(|claim| {
                    record.windows.iter().any(|w| {
                        w.kind == crate::model::WindowClaimKind::Product
                            && w.hwnd == claim.hwnd
                            && w.process == claim.process
                            && w.tag == claim.tag
                    })
                });
            if !exact {
                return Err(err("error: hide-proof ledger receipt mismatch"));
            }
        }
        _ => return Err(err("error: hide-proof ledger receipt missing")),
    }
    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs(s));
    loop {
        if stop_requested(dir, me)? {
            return Ok(());
        }
        if deadline.is_some_and(|end| Instant::now() >= end) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}
