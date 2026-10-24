#![forbid(unsafe_code)]
use super::*;

/// `tile` command: normal user tiling only (explicit `--user-start`).
/// Geometry is left in place on stop; nothing is hidden or restored.
/// Terminal windows are ordinary targets; agents never run this path.
/// Optional repeatable `--scope-exe NAME` restricts management to the named
/// executables (empty default means no filter). Product keyboard takeover
/// follows the parsed options (default on, visible off switch, Win+L
/// opt-in). Session-only mouse-Snap prevention is default on with the
/// visible `--no-mouse-snap-prevention` off switch; the exact preimage is
/// restored conditionally on stop.
pub fn cmd_tile(
    options: &TileOptions,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
    cli_overrides: crate::tiling::CliOverrides,
) -> Result<String> {
    if !options.user_start {
        return Err(err("refuse: tile requires explicit --user-start"));
    }
    // The first-run preset resolves under the owner lease inside the loop
    // below (never before it): a refused second owner never prompts, and a
    // mid-prompt file race reloads instead of overwriting. These pre-lease
    // values are the no-prompt fallback; the lease-held resolution may
    // replace every derived lane before any effect or hook.
    let trace = options.trace;
    let seconds = options.seconds;
    let keyboard = KeyboardConfig {
        takeover: !options.no_keyboard_snap_takeover,
        allow_win_l: options.allow_win_l,
    };
    let mouse_snap = !options.no_mouse_snap_prevention;
    let scope = options.scope_exes.clone();
    let scope_hosts = options.scope_hosts.clone();
    let border = options.border;
    let pager_highlight = options.pager_highlight;
    let underlay = options.underlay;
    let inner_gap = options.inner_gap;
    let outer_gap = options.outer_gap;
    let options = options.clone();
    let pending = FirstRunPending {
        options,
        live: live.clone(),
        settings_dir: settings_dir.clone(),
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: None,
                proof: false,
                shortcut_proof: false,
                workspace_proof: false,
                raw_argv: Vec::new(),
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: Some(pending),
                inner_gap,
                outer_gap,
                settings_dir,
                settings_live: live,
                cli_overrides,
                scope,
                scope_hosts,
                border,
                pager_highlight,
                underlay,
            },
        )
    })
}

/// Load the normal-owner settings base: persisted settings supply the
/// `TileOptions` defaults that explicit CLI switches override. Proof owners
/// never call this (deterministic explicit configuration only). Returns the
/// base options plus the initialized last-good live state and directory for
/// pump polling (`None` live state disables polling).
pub fn load_normal_tile_base() -> (
    TileOptions,
    Option<LiveSettings>,
    Option<std::path::PathBuf>,
) {
    let defaults = crate::tiling::tile_options_defaults();
    let Ok(dir) = crate::native::settings_directory() else {
        return (defaults, None, None);
    };
    let mtime = std::fs::metadata(dir.join(crate::settings::SETTINGS_FILE_NAME))
        .and_then(|meta| meta.modified())
        .ok();
    match crate::settings::load_from_dir(&dir) {
        crate::settings::LoadOutcome::Loaded(settings) => {
            let options = crate::settings::tile_options_from_settings(&settings);
            (
                options,
                Some(LiveSettings::fresh(settings, mtime)),
                Some(dir),
            )
        }
        crate::settings::LoadOutcome::Missing => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = "missing: defaults".to_owned();
            (defaults, Some(live), Some(dir))
        }
        crate::settings::LoadOutcome::Invalid(error) => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = format!("degraded:{error}");
            (defaults, Some(live), Some(dir))
        }
    }
}

/// `workspace` command: exact-owner out-of-hook control for the normal `tile`
/// loop only. Queues one bounded `workspace.request` file; the owner validates
/// the full owner binding (creation/pid/exe/sid/session plus a client
/// correlation) and dispatches once through the existing `workspace_do_select`
/// / `workspace_do_send` resolvers (or the existing project-owned fullscreen
/// toggle for `--fullscreen`). No window actuation here, no synthetic input,
/// acceptance. Refuses when no normal owner runs, when the caller is not the
/// same medium path (SID/session/exe), when the ledger owner is a proof run,
/// and when a request is already pending (single-pending queue, no overwrite,
/// no replay). Stdout reports `dispatched` (queued) honestly; completion is
/// the owner's `workspace` (or `fullscreen-toggle` for `--fullscreen`) log
/// outcome, observed natively by the caller.
pub fn cmd_workspace(options: &WorkspaceOptions) -> Result<String> {
    use crate::tiling::{WORKSPACE_REQUEST_VERSION, WorkspaceRequest, render_workspace_request};
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if options.index > 9 {
        return Err(err("refuse: workspace index must be 0..=9"));
    }
    let path = dir.join(WORKSPACE_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: workspace request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = WorkspaceRequest {
        v: WORKSPACE_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        action: options.action,
        index: options.index,
        direction: options.direction,
        correlation: correlation.clone(),
    };
    let body = render_workspace_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        WORKSPACE_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: workspace request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": options.action.as_str(), "index": options.index, "direction": options.direction.map(|d| d.as_str())}).to_string())
}

/// Test-needed exact-owner keyboard-resize control for the normal `tile`
/// loop only (tentative, pending user review; the product hook still filters
/// injected chords, so synthetic input can never drive a resize). Mirrors
/// `cmd_workspace` exactly: same-path medium caller, exact ledger-owner
/// liveness, proof-owner refusal, and a bounded single-pending request file
/// (`resize.request`). The owner consumes the request once, resolves the
/// live foreground as the resize subject through the production origin/
/// member/lifetime/scope gates, and dispatches through the real
/// `keyboard_tick` resize arm: no classifier bypass, no fence bypass. The
/// CLI carries direction/mode only, never an HWND.
pub fn cmd_resize(options: &ResizeOptions) -> Result<String> {
    use crate::tiling::{RESIZE_REQUEST_VERSION, ResizeRequest, render_resize_request};
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if !crate::tiling::resize_direction_valid(&options.direction) {
        return Err(err("refuse: resize direction must be left|right|up|down"));
    }
    if !crate::tiling::resize_mode_valid(&options.mode) {
        return Err(err("refuse: resize mode must be outwards|inwards"));
    }
    let path = dir.join(RESIZE_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: resize request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = ResizeRequest {
        v: RESIZE_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        direction: options.direction.clone(),
        mode: options.mode.clone(),
        correlation: correlation.clone(),
    };
    let body = render_resize_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        RESIZE_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: resize request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": "resize", "direction": options.direction, "mode": options.mode, "route": "test-needed-tentative"}).to_string())
}

/// Test-needed exact-owner directional move/focus control for the normal
/// `tile` loop only (tentative, pending user review; the product hook still
/// filters injected chords, so synthetic input can never drive a move or a
/// focus). Mirrors `cmd_resize` exactly: same-path medium caller, exact
/// ledger-owner liveness, proof-owner refusal, and a bounded single-pending
/// request file (`directional.request`). The owner consumes the request once,
/// resolves the live foreground as the subject through the production
/// origin/member/lifetime/scope gates, and dispatches through the real
/// `keyboard_tick` directional arm without changing the hook classifier.
/// The CLI carries op/direction only, never an HWND.
pub fn cmd_directional(options: &DirectionalOptions) -> Result<String> {
    use crate::tiling::{
        DIRECTIONAL_REQUEST_VERSION, DirectionalRequest, render_directional_request,
    };
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if !crate::tiling::resize_direction_valid(&options.direction) {
        return Err(err(
            "refuse: directional direction must be left|right|up|down",
        ));
    }
    let path = dir.join(DIRECTIONAL_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: directional request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = DirectionalRequest {
        v: DIRECTIONAL_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        op: options.op,
        direction: options.direction.clone(),
        correlation: correlation.clone(),
    };
    let body = render_directional_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        DIRECTIONAL_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: directional request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": options.op.as_str(), "direction": options.direction, "route": "test-needed-tentative"}).to_string())
}

/// Test-needed exact-owner explicit output-send control for the normal
/// `tile` loop only (tentative, pending user review; the product hook still
/// filters injected chords, so synthetic input can never drive a send).
/// Mirrors `cmd_directional` exactly: same-path medium caller, exact
/// ledger-owner liveness, proof-owner refusal, and a bounded single-pending
/// request file (`send-output.request`). The owner consumes the request once,
/// resolves the live foreground as the subject through the production
/// origin/member/lifetime/scope gates, and dispatches through the real
/// explicit `SendToOutput` production route without changing the hook
/// classifier. The CLI carries follow/direction only, never an HWND.
pub fn cmd_send_output(options: &SendOutputOptions) -> Result<String> {
    use crate::tiling::{
        SEND_OUTPUT_REQUEST_VERSION, SendOutputRequest, render_send_output_request,
    };
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if !crate::tiling::resize_direction_valid(&options.direction) {
        return Err(err(
            "refuse: send-output direction must be left|right|up|down",
        ));
    }
    let path = dir.join(SEND_OUTPUT_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: send-output request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = SendOutputRequest {
        v: SEND_OUTPUT_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        follow: options.follow,
        direction: options.direction.clone(),
        correlation: correlation.clone(),
    };
    let body = render_send_output_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        SEND_OUTPUT_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: send-output request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": if options.follow { "follow" } else { "stay" }, "direction": options.direction, "route": "test-needed-tentative"}).to_string())
}

/// Test-needed exact-owner whole-workspace migration control for the normal
/// `tile` loop only (tentative, pending user review; the product hook still
/// filters injected chords, so synthetic input can never drive a migration).
/// Mirrors `cmd_send_output` exactly: same-path medium caller, exact
/// ledger-owner liveness, proof-owner refusal, and a bounded single-pending
/// request file (`migrate-workspace.request`). The owner consumes the request
/// once and dispatches through the real production migration route without
/// changing the hook classifier. The CLI carries direction only, never an
/// HWND.
pub fn cmd_migrate_workspace(options: &MigrateWorkspaceOptions) -> Result<String> {
    use crate::tiling::{
        MIGRATE_WORKSPACE_REQUEST_VERSION, MigrateWorkspaceRequest,
        render_migrate_workspace_request,
    };
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if !crate::tiling::resize_direction_valid(&options.direction) {
        return Err(err(
            "refuse: migrate-workspace direction must be left|right|up|down",
        ));
    }
    let path = dir.join(MIGRATE_WORKSPACE_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: migrate-workspace request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = MigrateWorkspaceRequest {
        v: MIGRATE_WORKSPACE_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        direction: options.direction.clone(),
        correlation: correlation.clone(),
    };
    let body = render_migrate_workspace_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        MIGRATE_WORKSPACE_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: migrate-workspace request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": "migrate-workspace", "direction": options.direction, "route": "test-needed-tentative"}).to_string())
}

/// `capture` command: read-only frozen-allowlist capture of explicitly listed
/// owned-helper windows only. Each `--hwnd` target is fully validated
/// (owned-helper identity with lifetime tag, proof-mode eligibility, fresh
/// rectangles from one coherent observe pair); any ineligible or untagged
/// target refuses the whole capture. The HWND pid is rechecked immediately
/// before the receipt write. Receipts may carry raw native ids; production
/// logs never do.
pub fn cmd_capture(options: &CaptureOptions) -> Result<String> {
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let areas = all_monitors()?;
    let fulls = monitor_fulls(&areas);
    let mut tokens = TokenMap::default();
    let mut entries = Vec::new();
    let mut guarded: Vec<(u64, u32)> = Vec::new();
    for hwnd_u64 in &options.hwnds {
        let hwnd = *hwnd_u64 as isize as HWND;
        let window = match observe_window(hwnd, &me, &fulls, &mut tokens) {
            Ok(window) if window.hwnd == *hwnd_u64 => window,
            _ => return Err(err("refuse: capture target ineligible")),
        };
        if window.identity.tag.is_empty() {
            return Err(err("refuse: capture target ineligible"));
        }
        // Owned-helper binding: sibling exe/class/tag. Generic windows never
        // capture even when otherwise eligible.
        let helper_exe =
            crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
        let snap = crate::test_window::sys::query_owned(
            window.hwnd,
            &helper_exe,
            &me.user_sid,
            me.session_id,
        )
        .map_err(|_| err("refuse: capture target ineligible"))?;
        if snap.tag != window.identity.tag || snap.tag.is_empty() {
            return Err(err("refuse: capture target ineligible"));
        }
        if classify(&window.facts).is_err() {
            return Err(err("refuse: capture target ineligible"));
        }
        guarded.push((window.hwnd, window.identity.pid));
        entries.push(serde_json::json!({
            "hwnd": window.hwnd,
            "pid": window.identity.pid,
            "process_creation": window.identity.process_creation,
            "exe_path": window.identity.exe_path,
            "user_sid": window.identity.user_sid,
            "session_id": window.identity.session_id,
            "tag": window.identity.tag,
            "outer": [window.outer.x, window.outer.y, window.outer.w, window.outer.h],
            "visible": [window.visible.x, window.visible.y, window.visible.w, window.visible.h],
        }));
    }
    if options.hwnds.is_empty() {
        return Err(err("refuse: capture requires at least one --hwnd"));
    }
    // Fresh per-HWND pid guard immediately before the write: a recycled
    // HWND between observation and receipt refuses the whole capture.
    for (hwnd_u64, pid) in &guarded {
        let current = crate::ffi::window_pid(*hwnd_u64);
        if current == 0 || current != *pid {
            return Err(err("refuse: capture target identity changed"));
        }
    }
    let receipt = serde_json::json!({ "windows": entries }).to_string();
    if options.out.exists() {
        return Err(err("refuse: receipt preexists"));
    }
    std::fs::write(&options.out, &receipt)
        .map_err(|e| err(format!("error: receipt write: {e}")))?;
    Ok(serde_json::json!({"captured": entries.len()}).to_string())
}

/// `inventory` command: read-only top-level window list for selecting capture
/// targets. Reports HWND, pid, executable basename, class, and style bits;
/// never reads titles. Stdout only; nothing is written or tiled.
pub fn cmd_inventory() -> Result<String> {
    ensure_pm_v2()?;
    let Some(hwnds) = enumerate_hwnds() else {
        return Err(err("error: enumeration failed"));
    };
    let mut entries = Vec::new();
    for raw in hwnds {
        let hwnd_u64 = raw as usize as u64;
        let hwnd = raw as HWND;
        if !crate::ffi::is_visible(hwnd_u64) {
            continue;
        }
        let pid = crate::ffi::window_pid(hwnd_u64);
        if pid == 0 {
            continue;
        }
        let exe = HeldProcess::open(pid)
            .ok()
            .and_then(|held| held.identity().ok())
            .map(|ident| {
                ident
                    .exe_path
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or("unknown")
                    .to_owned()
            })
            .unwrap_or_else(|| "unknown".to_owned());
        let style = crate::ffi::window_style(hwnd_u64);
        let exstyle = crate::ffi::window_exstyle(hwnd_u64);
        entries.push(serde_json::json!({
            "hwnd": raw as u64,
            "pid": pid,
            "exe": exe,
            "class": class_of(hwnd),
            "style": format!("{style:#x}"),
            "exstyle": format!("{exstyle:#x}"),
        }));
    }
    Ok(serde_json::json!({ "windows": entries }).to_string())
}
