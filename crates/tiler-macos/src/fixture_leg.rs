#![forbid(unsafe_code)]

//! Development-only fixture leg runner: the complete real driver entry
//! point from baseline through owned shutdown.
//!
//! `fixture-leg` owns the whole lifecycle in one controller: capture the
//! full baseline before ANY setup effect, launch the signed fixture bundle
//! through LaunchServices into a private run directory, bind its
//! announcement (socket, PID, nonce) plus executable/launch-date/code
//! signature, spawn the fixture-scoped product as an exact held `Child`,
//! prove the return route on a newly created owned Space, drive the
//! requested semantic chords through the admitted driver, then tear down in
//! order (exact product stop first, starting-Space restore, owned window
//! closes, identity-verified fixture shutdown and reap, owned empty-Space
//! removal, awake stop, final capture and compare).
//!
//! Fixture-only scope is fixed: exactly `io.github.omnitiler.live-fixture`,
//! never another scope, never unscoped. The single-shot external
//! `fixture-post`/`fixture-focus` commands (no controller lifecycle, no
//! registry) refuse rather than pretend a clean leg; the usable repeated
//! driver path lives here.

use crate::config::ProbeModifier;
use crate::input_policy::InputPreset;
use crate::live_leg::{
    AdmissionScope, FixtureRecord, LiveLeg, LiveLegTransport, ProductRecord, required_scope,
};
use crate::space_probe::NativeSpaces;

/// Exact fixture bundle scope for fixture-only legs. No other app scope.
pub const FIXTURE_SCOPE: &str = "io.github.omnitiler.live-fixture";

/// Fixture bundle leaf under the repository target directory (dev-only
/// build layout, assembled by the fixture-bundle recipe). Descriptive only:
/// resolution is executable-anchored (see [`FIXTURE_BUNDLE_FILE_NAME`]) and
/// never joins the process working directory.
pub const FIXTURE_BUNDLE_LEAF: &str = "target/macos/OmniTilerLiveFixture.app";

/// Fixture bundle file name: the sibling of the probe bundle under the same
/// parent directory (dev-only build layout, assembled by the fixture-bundle
/// recipe). The controller resolves it next to its own signed bundle root,
/// so a LaunchServices launch (whose working directory is not the
/// repository) finds the same signed layout.
pub const FIXTURE_BUNDLE_FILE_NAME: &str = "OmniTilerLiveFixture.app";

/// Owned-bundle marker inside the fixture bundle (same shape as the
/// fixture-bundle recipe's fixed-bundle guard).
pub const FIXTURE_MARKER_REL: &str = "Contents/Resources/.omnitiler-fixture-marker";

/// Bundle executable inside the fixture bundle.
pub const FIXTURE_EXE_REL: &str = "Contents/MacOS/tiler-live-fixture";

/// Environment override for the fixture bundle path (development only).
/// The bind still enforces the exact bundle scope plus the code signature,
/// so an override can never widen the leg's authority.
pub const FIXTURE_APP_ENV: &str = "OMNITILER_FIXTURE_APP";

/// Bounded wait for the LaunchServices announcement file in seconds
/// (genuine deadline: `open(1)` offers no readiness event; stated here).
pub const ANNOUNCE_WAIT_SECS: u64 = 15;

/// Bounded wait for the LaunchServices product `tilestarted` line in
/// seconds (genuine deadline: `open(1)` offers no readiness event and the
/// product owns its startup latency; stated here).
pub const PRODUCT_WAIT_SECS: u64 = 30;

/// Leg bounds in seconds: 60..=MAX_TILE_SECONDS (reused, never a hardcoded
/// mismatch: the leg seconds ride verbatim into the product `--seconds`
/// flag, so the leg max is exactly what the product accepts).
pub const LEG_MIN_SECONDS: u64 = 60;
/// Leg bounds in seconds: 60..=MAX_TILE_SECONDS (reused, never a hardcoded
/// mismatch: the leg seconds ride verbatim into the product `--seconds`
/// flag, so the leg max is exactly what the product accepts).
pub const LEG_MAX_SECONDS: u64 = crate::tile::MAX_TILE_SECONDS;

/// Maximum chords per leg invocation (bounded automation, never open-ended).
pub const LEG_MAX_CHORDS: usize = 32;

/// Operator settle window (harness only): milliseconds the leg holds before
/// its first chord and again after its last chord, so an operator can drive
/// extra owned fixture windows over the announced control socket and read
/// native frames while the product is running. Absent or `0` leaves the
/// leg's own timing untouched. Never a product flag: this only spaces the
/// harness's chord posts.
pub const SETTLE_ENV: &str = "OMNITILER_FIXTURE_LEG_SETTLE_MS";

/// Upper bound for one operator settle window (milliseconds).
pub const SETTLE_MAX_MS: u64 = 20_000;

/// Shutdown margin required on top of both settle windows, so the product's
/// own bounded lifetime never expires mid-hold.
pub const SETTLE_LIFETIME_MARGIN_MS: u64 = 10_000;

/// One fixture-leg invocation.
pub struct FixtureLegOpts {
    pub seconds: u64,
    pub chords: Vec<String>,
    pub preset: InputPreset,
    pub modifier: ProbeModifier,
    /// Expected fixture designated requirement (from the fixture-bundle
    /// verification output at preflight; machine-specific, never hardcoded
    /// here). Empty refuses before any post.
    pub signer: String,
    /// Forward the tile command's CLI-only focus-follows-cursor override.
    pub focus_follows_cursor: bool,
    /// Run the opt-in bounded native hover proof after the chords
    /// (`fixture_hover::run`). Requires at least
    /// [`crate::fixture_hover::HOVER_MIN_WINDOWS`] chords and launches the
    /// product with stage-timing diagnostics; default off.
    pub hover_probe: bool,
}

/// Own signed bundle root for a controller executable running from the
/// assembled bundle layout (`<name>.app/Contents/MacOS/<bin>`). Pure;
/// unit-tested. Returns `None` for any other layout (developer `cargo run`,
/// test binaries): callers fail closed rather than guessing from the working
/// directory (which LaunchServices does not set to the repository).
fn own_bundle_root(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let root = contents.parent()?;
    if macos.file_name()? != std::ffi::OsStr::new("MacOS") {
        return None;
    }
    if contents.file_name()? != std::ffi::OsStr::new("Contents") {
        return None;
    }
    if root.extension()? != std::ffi::OsStr::new("app") {
        return None;
    }
    Some(root.to_path_buf())
}

/// Verify the fixed fixture artifact before any launch: the owned marker
/// names exactly the fixture scope and the executable exists without
/// symlinks. Mirrors the fixture-bundle recipe's fixed-bundle guard; refuses
/// foreign bundles before any effect.
fn verify_fixture_artifact(bundle: &std::path::Path) -> Result<(), String> {
    let marker = bundle.join(FIXTURE_MARKER_REL);
    match std::fs::symlink_metadata(&marker) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("fixture-leg: fixture marker is a symlink; refusing".to_owned());
        }
        Ok(meta) if meta.is_file() => {}
        _ => {
            return Err(
                "fixture-leg: fixture bundle lacks the owned marker; refusing foreign bundle"
                    .to_owned(),
            );
        }
    }
    let raw = std::fs::read_to_string(&marker)
        .map_err(|_| "fixture-leg: fixture marker unreadable; refusing".to_owned())?;
    if raw.lines().next().unwrap_or("") != FIXTURE_SCOPE {
        return Err("fixture-leg: fixture marker mismatch; refusing foreign bundle".to_owned());
    }
    let exe = bundle.join(FIXTURE_EXE_REL);
    match std::fs::symlink_metadata(&exe) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err("fixture-leg: fixture executable is a symlink; refusing".to_owned())
        }
        Ok(meta) if meta.is_file() => Ok(()),
        _ => Err("fixture-leg: fixture executable missing; refusing".to_owned()),
    }
}

/// Check one fixture bundle directory: never a symlink, always a directory
/// carrying the owned marker plus executable (see
/// [`verify_fixture_artifact`]). Shared by the override and the
/// executable-anchored default, so an override can never widen the leg's
/// authority.
fn check_fixture_bundle(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(
            "fixture-leg: fixture bundle path is a symlink; refusing".to_owned(),
        ),
        Ok(meta) if meta.is_dir() => verify_fixture_artifact(path).map(|()| path.to_path_buf()),
        Ok(_) => Err("fixture-leg: fixture bundle path is not a directory; refusing".to_owned()),
        Err(_) => Err(
            "fixture-leg: fixture bundle missing (build it first with the fixture-bundle recipe); refusing"
                .to_owned(),
        ),
    }
}

/// Resolve the fixture bundle directory: the environment override when set,
/// else the sibling of the controller's own signed bundle root. The working
/// directory is never consulted: under LaunchServices it is not the
/// repository. The path must be an existing non-symlink directory carrying
/// the fixture marker; the exact scope plus signature bind happens later and
/// refuses anything but the signed fixture bundle. Pure in `exe` plus the
/// override (see [`fixture_bundle_path`]); unit-tested.
fn resolve_fixture_bundle(
    exe: &std::path::Path,
    override_path: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    if let Some(path) = override_path.filter(|path| !path.is_empty()) {
        return check_fixture_bundle(std::path::Path::new(path));
    }
    let root = own_bundle_root(exe).ok_or_else(|| {
        "fixture-leg: controller is not running from the signed bundle layout (launch through the signed probe bundle or set OMNITILER_FIXTURE_APP); refusing"
            .to_owned()
    })?;
    let parent = root.parent().ok_or_else(|| {
        "fixture-leg: signed bundle root has no parent directory; refusing".to_owned()
    })?;
    check_fixture_bundle(&parent.join(FIXTURE_BUNDLE_FILE_NAME))
}

/// Resolve the fixture bundle directory: the environment override when set,
/// else the sibling of the controller's own signed bundle root (see
/// [`resolve_fixture_bundle`]).
fn fixture_bundle_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("fixture-leg: executable path unreadable ({e})"))?;
    let override_path = std::env::var(FIXTURE_APP_ENV).ok();
    resolve_fixture_bundle(&exe, override_path.as_deref())
}

/// Fixed signed product bundle leaf under the repository target directory
/// (dev-only build layout, assembled by the probe-bundle recipe).
/// Descriptive only: resolution is executable-anchored (see
/// [`PRODUCT_BUNDLE_FILE_NAME`]) and never joins the process working
/// directory.
pub const PRODUCT_BUNDLE_LEAF: &str = "target/macos/OmniTilerProbe.app";

/// Product bundle file name: the controller's own signed bundle root when it
/// runs from the assembled layout (see [`own_bundle_root`).
pub const PRODUCT_BUNDLE_FILE_NAME: &str = "OmniTilerProbe.app";

/// Owned-bundle marker inside the product bundle (same shape as the
/// launch script's fixed-bundle guard).
pub const PRODUCT_MARKER_REL: &str = "Contents/Resources/.omnitiler-probe-marker";

/// Bundle executable inside the product bundle.
pub const PRODUCT_EXE_REL: &str = "Contents/MacOS/tiler-macos";

/// Environment override for the product bundle path (development only).
/// The bind still enforces the exact probe bundle scope plus the marker,
/// so an override can never widen the leg's authority.
pub const PRODUCT_APP_ENV: &str = "OMNITILER_PROBE_APP";

/// Check one product bundle directory: never a symlink, always a directory
/// carrying the owned marker plus executable (see
/// [`verify_product_artifact`]). Shared by the override and the
/// executable-anchored default, so an override can never widen the leg's
/// authority.
fn check_product_bundle(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(
            "fixture-leg: product bundle path is a symlink; refusing".to_owned(),
        ),
        Ok(meta) if meta.is_dir() => verify_product_artifact(path).map(|()| path.to_path_buf()),
        Ok(_) => {
            Err("fixture-leg: product bundle path is not a directory; refusing".to_owned())
        }
        Err(_) => Err(
            "fixture-leg: product bundle missing (build it first with the probe-bundle recipe); refusing"
                .to_owned(),
        ),
    }
}

/// Resolve the product bundle directory: the environment override when set,
/// else the controller's own signed bundle root. The working directory is
/// never consulted: under LaunchServices it is not the repository. The
/// controller itself runs from the probe bundle, so the own root is the
/// product bundle; the owned-marker check refuses any foreign host bundle
/// before any effect. Pure in `exe` plus the override (see
/// [`product_bundle_path`]); unit-tested.
fn resolve_product_bundle(
    exe: &std::path::Path,
    override_path: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    if let Some(path) = override_path.filter(|path| !path.is_empty()) {
        return check_product_bundle(std::path::Path::new(path));
    }
    let root = own_bundle_root(exe).ok_or_else(|| {
        "fixture-leg: controller is not running from the signed bundle layout (launch through the signed probe bundle or set OMNITILER_PROBE_APP); refusing"
            .to_owned()
    })?;
    check_product_bundle(&root)
}

/// Resolve the product bundle directory: the environment override when set,
/// else the controller's own signed bundle root (see
/// [`resolve_product_bundle`]). The leg launches the product through
/// LaunchServices from this bundle (never a direct executable spawn, which
/// would lose the signed attribution).
fn product_bundle_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("fixture-leg: executable path unreadable ({e})"))?;
    let override_path = std::env::var(PRODUCT_APP_ENV).ok();
    resolve_product_bundle(&exe, override_path.as_deref())
}

/// Verify the fixed product artifact before any launch: the owned marker
/// names exactly the probe bundle ID and the executable exists without
/// symlinks. Mirrors the launch script's fixed-bundle guard; refuses
/// foreign bundles before any effect.
fn verify_product_artifact(bundle: &std::path::Path) -> Result<(), String> {
    let marker = bundle.join(PRODUCT_MARKER_REL);
    match std::fs::symlink_metadata(&marker) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("fixture-leg: product marker is a symlink; refusing".to_owned());
        }
        Ok(meta) if meta.is_file() => {}
        _ => {
            return Err(
                "fixture-leg: product bundle lacks the owned marker; refusing foreign bundle"
                    .to_owned(),
            );
        }
    }
    let raw = std::fs::read_to_string(&marker)
        .map_err(|_| "fixture-leg: product marker unreadable; refusing".to_owned())?;
    if raw.lines().next().unwrap_or("") != crate::BUNDLE_ID {
        return Err("fixture-leg: product marker mismatch; refusing foreign bundle".to_owned());
    }
    let exe = bundle.join(PRODUCT_EXE_REL);
    match std::fs::symlink_metadata(&exe) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err("fixture-leg: product executable is a symlink; refusing".to_owned())
        }
        Ok(meta) if meta.is_file() => Ok(()),
        _ => Err("fixture-leg: product executable missing; refusing".to_owned()),
    }
}

/// Create an owned private run directory: fresh unique leaf, mode 0700,
/// never a symlink, owned by the current UID. Refuses on any deviation.
fn create_owned_run_dir(tag: &str) -> Result<std::path::PathBuf, String> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("fixture-leg-{tag}-{}-{id}", std::process::id()));
    match std::fs::symlink_metadata(&dir) {
        Ok(_) => {
            return Err("fixture-leg: run directory already exists; refusing".to_owned());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("fixture-leg: run directory metadata refused ({e})")),
    }
    std::fs::create_dir(&dir).map_err(|e| format!("fixture-leg: run directory refused ({e})"))?;
    crate::desktop_shortcuts::ensure_private_dir(&dir)?;
    Ok(dir)
}

/// Parse and bound one operator settle window from its raw environment
/// value. Pure; unit-tested. `None`, empty, or `0` means no settle. The
/// window must leave the product's own bounded lifetime intact (both holds
/// plus one shutdown margin), so a settle can never outlive the run it is
/// observing.
fn settle_ms_from(raw: Option<&str>, seconds: u64) -> Result<u64, String> {
    let Some(raw) = raw else { return Ok(0) };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(0);
    }
    let value: u64 = raw.parse().map_err(|_| {
        format!("fixture-leg: {SETTLE_ENV} must be whole milliseconds (got '{raw}')")
    })?;
    if value == 0 {
        return Ok(0);
    }
    if value > SETTLE_MAX_MS {
        return Err(format!(
            "fixture-leg: {SETTLE_ENV} {value}ms exceeds the {SETTLE_MAX_MS}ms settle bound; refusing"
        ));
    }
    if value
        .saturating_mul(2)
        .saturating_add(SETTLE_LIFETIME_MARGIN_MS)
        > seconds.saturating_mul(1000)
    {
        return Err(format!(
            "fixture-leg: {SETTLE_ENV} {value}ms twice plus a {SETTLE_LIFETIME_MARGIN_MS}ms margin does not fit the {seconds}s product lifetime; refusing"
        ));
    }
    Ok(value)
}

/// Read the operator settle window from the environment (harness only).
fn settle_ms(seconds: u64) -> Result<u64, String> {
    settle_ms_from(std::env::var(SETTLE_ENV).ok().as_deref(), seconds)
}

/// Hold for one settle window, polling the interrupt latch so SIGINT/SIGTERM
/// still unwinds through the leg's full teardown.
#[cfg(target_os = "macos")]
fn settle_hold(ms: u64) -> Result<(), (i32, String)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err((
                1,
                "fixture-leg: interrupted (SIGINT/SIGTERM latched); restoring".to_owned(),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

/// Launch the signed fixture bundle through LaunchServices into the owned
/// run directory. Returns when `open(1)` spawns (LaunchServices owns the
/// process from here; no `Child` is held, so lifetime rides the bound
/// executable/launch-date plus the run nonce).
fn launch_fixture_bundle(
    bundle: &std::path::Path,
    run_dir: &std::path::Path,
) -> Result<(), String> {
    let announcement = run_dir.join("announcement.json");
    let stderr_log = run_dir.join("stderr.log");
    let status = std::process::Command::new("open")
        .args([
            "-n",
            "--stdout",
            &announcement.to_string_lossy(),
            "--stderr",
            &stderr_log.to_string_lossy(),
        ])
        .arg(bundle)
        .args(["--args", "serve", "--dir"])
        .arg(run_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("fixture-leg: fixture launch spawn refused ({e})"))?;
    if !status.success() {
        return Err(format!(
            "fixture-leg: fixture launch spawn refused (open exit {})",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

/// Read the single announcement line with a genuine deadline (`open(1)`
/// offers no readiness event; stated here where the timer is defined).
fn read_announcement(run_dir: &std::path::Path) -> Result<String, String> {
    let path = run_dir.join("announcement.json");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(ANNOUNCE_WAIT_SECS);
    loop {
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                for line in raw.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        return Ok(line.to_owned());
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("fixture-leg: announcement file refused ({e})")),
        }
        if std::time::Instant::now() >= deadline {
            return Err(
                "fixture-leg: announcement unreadable past the launch deadline; refusing"
                    .to_owned(),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Build the exact product argv for a LaunchServices launch: the fixed
/// fixture-only scope, the leg seconds verbatim (always within
/// [`LEG_MIN_SECONDS`]..=[`LEG_MAX_SECONDS`], hence product-accepted), the
/// tap route, the leg preset, the exact-run directory, plus the
/// controller-owned gate SIDs (repeatable `--fixture-owned-space`) and the
/// recorded starting current. Pure: the launch-args test pins the fixed
/// scope plus private gate IDs (never `--unscoped`, never another scope).
fn product_argv(
    opts: &FixtureLegOpts,
    product_dir: &std::path::Path,
    owned_sids: &[u64],
    starting_sid: Option<u64>,
) -> Vec<String> {
    let mut argv = vec![
        "tile".to_owned(),
        "--user-start".to_owned(),
        "--scope".to_owned(),
        FIXTURE_SCOPE.to_owned(),
        "--seconds".to_owned(),
        opts.seconds.to_string(),
        "--input".to_owned(),
        "tap".to_owned(),
        "--preset".to_owned(),
        opts.preset.as_str().to_owned(),
        "--run-dir".to_owned(),
        product_dir.to_string_lossy().into_owned(),
    ];
    if opts.focus_follows_cursor {
        argv.push("--focus-follows-cursor".to_owned());
    }
    for sid in owned_sids {
        argv.push("--fixture-owned-space".to_owned());
        argv.push(sid.to_string());
    }
    if let Some(starting) = starting_sid {
        argv.push("--fixture-starting-space".to_owned());
        argv.push(starting.to_string());
    }
    argv.insert(0, format!("--ctrl={}", opts.modifier.as_str()));
    argv
}

/// Read the product `tilestarted` announcement (PID plus run ID) from the
/// LaunchServices stdout log: the first `tilestarted:true` JSON line.
/// `None` while unwritten or unparsable (the caller polls under its own
/// genuine deadline). Pure file shape; unit-tested.
fn read_product_started(stdout_log: &std::path::Path) -> Option<(u32, String)> {
    let raw = std::fs::read_to_string(stdout_log).ok()?;
    raw.lines().find_map(|line| {
        let value = serde_json::from_str::<serde_json::Value>(line.trim()).ok()?;
        if value.get("tilestarted") != Some(&serde_json::Value::Bool(true)) {
            return None;
        }
        let pid = value
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 1)?;
        let announced = value
            .get("runid")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)?;
        Some((pid, announced))
    })
}

/// Launch the signed product bundle through LaunchServices with the exact
/// `tile` argv, then bind the `tilestarted` announcement (PID plus run ID)
/// plus the running bundle/executable/launch identity. `open(1)` owns no
/// product lifetime (no `Child` is held: the wrapper exit is never native
/// success), so the returned record binds the LaunchServices lifetime for
/// the leg's identity-verified stop. Never a direct executable spawn (which
/// would lose the signed TCC attribution), never a PID/name guess: the PID
/// is fresh (absent before launch), the run ID equals this run's, and the
/// bundle/executable/launch-date join is live.
#[cfg(target_os = "macos")]
fn launch_product_ls(
    bundle: &std::path::Path,
    product_dir: &std::path::Path,
    argv: &[String],
    run_id: &str,
    hover_probe: bool,
) -> Result<ProductRecord, String> {
    use std::collections::BTreeSet;
    // PIDs of this bundle before launch: the announced PID must be fresh.
    let before: BTreeSet<i32> = crate::ffi::running_apps()
        .into_iter()
        .filter_map(|(pid, bundle_id)| (bundle_id == crate::BUNDLE_ID).then_some(pid))
        .collect();
    let stdout_log = product_dir.join("stdout.log");
    let stderr_log = product_dir.join("stderr.log");
    let mut open = std::process::Command::new("open");
    open.arg("-n")
        .arg("-o")
        .arg(&stdout_log)
        .arg("--stderr")
        .arg(&stderr_log);
    // Opt-in hover probe: stage timing plus the cause tokens ride the
    // product's own environment for this launch only (adapter-only, never a
    // settings write). Default runs keep the unchanged environment.
    if hover_probe {
        open.arg("--env")
            .arg(crate::fixture_hover::STAGE_TIMING_ENV);
    }
    // Fixture-scoped durable intent isolation: never read or write the
    // user's real Application Support store during a bounded leg. The
    // product creates the 0700 private directory under this run.
    open.arg("--env").arg(format!(
        "{}={}",
        crate::intent_store::STORE_DIR_ENV,
        product_dir.join("intent-store").display()
    ));
    let status = open
        .arg(bundle)
        .arg("--args")
        .args(argv)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("fixture-leg: product launch spawn refused ({e})"))?;
    if !status.success() {
        return Err(format!(
            "fixture-leg: product launch spawn refused (open exit {})",
            status.code().unwrap_or(-1)
        ));
    }
    // Bind the run-specific announcement: the first `tilestarted` JSON
    // line (genuine deadline: `open(1)` offers no readiness event and the
    // product owns its startup latency; stated here).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(PRODUCT_WAIT_SECS);
    let (pid, announced_run) = loop {
        if let Some(found) = read_product_started(&stdout_log) {
            break found;
        }
        if std::time::Instant::now() >= deadline {
            return Err(
                "fixture-leg: product tilestarted unreadable past the launch deadline; refusing"
                    .to_owned(),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    if announced_run != run_id {
        return Err(
            "fixture-leg: product run ID mismatch (stale or foreign run); refusing".to_owned(),
        );
    }
    if before.contains(&i32::try_from(pid).unwrap_or(0)) {
        return Err(
            "fixture-leg: product PID predates the launch (stale instance, not this run); refusing"
                .to_owned(),
        );
    }
    // Running identity: exact probe bundle scope plus a live executable
    // plus a LaunchServices launch date (direct-binary launches carry no
    // bundle identity and refuse here).
    let identity = crate::fixture_post::fixture_process_identity(pid)
        .map_err(|e| format!("fixture-leg: product identity unreadable ({e})"))?;
    match identity.bundle.as_deref() {
        Some(bundle_id) if bundle_id == crate::BUNDLE_ID => {}
        _ => {
            return Err(
                "fixture-leg: live product bundle is not the signed probe scope; refusing"
                    .to_owned(),
            );
        }
    }
    let (exe, launch) = match (identity.exe, identity.launch) {
        (Some(exe), Some(launch)) if !exe.is_empty() && !launch.is_empty() => (exe, launch),
        _ => {
            return Err(
                "fixture-leg: product executable or launch date unreadable (no LaunchServices lifetime); refusing"
                    .to_owned(),
            );
        }
    };
    // The live executable must be this bundle's executable (exact path or
    // canonical equivalent, never a foreign binary under our bundle ID).
    let expected_exe = bundle.join(PRODUCT_EXE_REL);
    let exe_matches = std::path::Path::new(&exe) == expected_exe.as_path()
        || (std::fs::canonicalize(&exe)
            .is_ok_and(|live| std::fs::canonicalize(&expected_exe).is_ok_and(|want| live == want)));
    if !exe_matches {
        return Err(
            "fixture-leg: product executable is not this bundle's executable; refusing".to_owned(),
        );
    }
    Ok(ProductRecord {
        pid,
        exe,
        launch,
        run_dir: product_dir.to_path_buf(),
        launch_services: true,
    })
}

/// What the leg body returns to the envelope: every chord driver report in
/// order, plus the opt-in hover-probe evidence and the established
/// post-only owned-Space evidence when it ran.
struct LegReports {
    chords: Vec<serde_json::Value>,
    hover_probe: Option<serde_json::Value>,
    owned_space: Option<serde_json::Value>,
}

/// Completed success envelope: the chord reports plus any established
/// owned-Space evidence. Pure over its inputs; unit-tested.
fn completed_envelope(
    chords: Vec<serde_json::Value>,
    hover_probe: Option<serde_json::Value>,
    owned_space: Option<serde_json::Value>,
    teardown: serde_json::Value,
) -> serde_json::Value {
    let mut envelope = serde_json::json!({
        "outcome": "completed",
        "chords": chords,
        "teardown": teardown,
    });
    if let Some(probe) = hover_probe {
        envelope["hover_probe"] = probe;
    }
    if let Some(owned) = owned_space {
        envelope["owned_space"] = owned;
    }
    envelope
}

/// Create AppKit's shared application singleton on the main thread.
///
/// `NSScreen.screensHaveSeparateSpaces` answers with a stale `false` until
/// that singleton exists (a fresh process prints `false` cold and `true`
/// after `NSApplication.shared`), so a baseline captured before the leg
/// initializes AppKit stores an untruthful preimage for an unchanged Mission
/// Control setting. The singleton handle is benign: no `finishLaunching`, no
/// activation policy change, no activation, and no focus effect.
#[cfg(target_os = "macos")]
fn ensure_appkit_singleton() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    if let Some(mtm) = MainThreadMarker::new() {
        let _ = NSApplication::sharedApplication(mtm);
    }
}

/// Development-only fixture leg runner: the complete real entry point.
/// Exit codes: 2 usage/validation, 1 live-gate/proof/teardown failure with
/// evidence. Every failure path runs the leg teardown (starting-Space
/// restore plus owned cleanup plus final comparison).
pub fn run_fixture_leg(opts: FixtureLegOpts) -> Result<String, (i32, String)> {
    if opts.seconds < LEG_MIN_SECONDS || opts.seconds > LEG_MAX_SECONDS {
        return Err((
            2,
            format!(
                "fixture-leg needs --seconds {LEG_MIN_SECONDS}..={LEG_MAX_SECONDS}, got {}",
                opts.seconds
            ),
        ));
    }
    if opts.chords.is_empty() || opts.chords.len() > LEG_MAX_CHORDS {
        return Err((
            2,
            format!(
                "fixture-leg needs 1..={LEG_MAX_CHORDS} --chord values, got {}",
                opts.chords.len()
            ),
        ));
    }
    for chord in &opts.chords {
        if crate::live_leg::required_scope(chord).is_none() {
            return Err((
                2,
                format!("fixture-leg: unknown --chord '{chord}' (see help)"),
            ));
        }
    }
    // The hover probe needs one tall anchor plus a stacked pair; refuse the
    // shape before any baseline, Space, or launch effect (no run dir).
    if opts.hover_probe && opts.chords.len() < crate::fixture_hover::HOVER_MIN_WINDOWS {
        return Err((
            2,
            format!(
                "fixture-leg --hover-probe needs at least {} --chord values (one tall anchor plus a stacked pair), got {}",
                crate::fixture_hover::HOVER_MIN_WINDOWS,
                opts.chords.len()
            ),
        ));
    }
    // Operator settle validation refuses before any baseline or launch: a
    // bad window must never leave a created run directory behind.
    settle_ms(opts.seconds).map_err(|e| (2, e))?;
    let _interrupt = crate::space_drag_diag_live::InterruptArmed::arm()
        .map_err(|e| (2, format!("fixture-leg: interrupt arm refused ({e})")))?;
    // The baseline precedes every effect, so the AppKit singleton must exist
    // before it: `NSScreen.screensHaveSeparateSpaces` answers with a stale
    // `false` default until the shared application exists, while the
    // teardown read (after the return probe and observation setup) sees the
    // real Mission Control setting. Initializing here keeps both edges of
    // the unchanged-setting comparison on the same truthful value; it does
    // not call `finishLaunching`, set an activation policy, activate, or
    // take focus.
    #[cfg(target_os = "macos")]
    ensure_appkit_singleton();
    let mut transport = LiveLegTransport::new();
    let mut leg = LiveLeg::begin(&mut transport, opts.seconds.saturating_add(60))?;
    // From here every failure runs the teardown: finish consumes the leg.
    let outcome = run_leg_body(&mut leg, &opts);
    match outcome {
        Ok(reports) => {
            let LegReports {
                chords,
                hover_probe,
                owned_space,
            } = reports;
            match leg.finish() {
                Ok(teardown) => {
                    let envelope = completed_envelope(
                        chords,
                        hover_probe,
                        owned_space,
                        serde_json::json!(teardown),
                    );
                    Ok(envelope.to_string())
                }
                Err(teardown) => {
                    // Chords were posted and proven before `finish`; keep
                    // their driver reports in the refusal so the completed
                    // posts stay auditable instead of being dropped with the
                    // teardown detail. The teardown body is already JSON;
                    // embed it as data when it parses (a malformed body stays
                    // a string).
                    let teardown_body = match serde_json::from_str::<serde_json::Value>(&teardown.1)
                    {
                        Ok(value) => value,
                        Err(_) => serde_json::Value::String(teardown.1),
                    };
                    let mut envelope = serde_json::json!({
                        "stage": "leg-finish",
                        "error": "chords posted but teardown refused",
                        "chords": chords,
                        "teardown": teardown_body,
                    });
                    if let Some(probe) = hover_probe {
                        envelope["hover_probe"] = probe;
                    }
                    if let Some(owned) = owned_space {
                        envelope["owned_space"] = owned;
                    }
                    Err((teardown.0, envelope.to_string()))
                }
            }
        }
        Err(error) => {
            if let Err(teardown) = leg.finish() {
                eprintln!("fixture-leg teardown after failure: {}", teardown.1);
            }
            Err(error)
        }
    }
}

/// The leg body between begin and finish: owned run directory, fixture
/// LaunchServices launch plus announcement bind plus registration, owned
/// window spawn plus native join plus registration, product spawn plus
/// registration, executed return probe for space-mutating chords, then the
/// admitted driver posts. Any failure returns before the caller runs the
/// teardown.
#[allow(clippy::too_many_lines)]
fn run_leg_body(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureLegOpts,
) -> Result<LegReports, (i32, String)> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (leg, opts);
        return Err((
            2,
            "fixture-leg needs macOS (LaunchServices launch plus native joins)".to_owned(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        run_leg_body_macos(leg, opts)
    }
}

/// Static Desktop-N ordinal for one space-mutating chord (`select-N` /
/// `send-N`: `0` is Desktop 10; relative/previous chords have no static
/// ordinal). Pure; unit-tested.
fn static_chord_ordinal(chord: &str) -> Option<u32> {
    let rest = chord
        .strip_prefix("select-")
        .or_else(|| chord.strip_prefix("send-"))?;
    if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
        let index = u32::from(rest.as_bytes()[0] - b'0');
        Some(if index == 0 { 10 } else { index })
    } else {
        None
    }
}

/// Plan the retained owned destinations for space-mutating chords: with a
/// baseline row of `baseline_count` native Spaces, static numbered chords
/// must address appended ordinary positions (`baseline_count + 1..`), never
/// user Spaces, and every position must stay within the Desktop-9 holder
/// mapping (position 10+ is unmappable: the send path refuses past
/// Desktop-9). Returns the owned ordinal range to append (count). Pure;
/// unit-tested.
fn plan_owned_destinations(
    chords: &[String],
    baseline_count: usize,
) -> Result<std::ops::RangeInclusive<usize>, String> {
    if baseline_count == 0 {
        return Err(
            "fixture-leg: baseline native row is empty; refusing without owned destinations"
                .to_owned(),
        );
    }
    if baseline_count >= 9 {
        return Err(format!(
            "fixture-leg: {baseline_count} user Spaces leave no representable ordinary target (holders map 1..=9); refusing instead of using user Spaces"
        ));
    }
    let mut static_ordinals: Vec<usize> = Vec::new();
    for chord in chords {
        if crate::live_leg::required_scope(chord) != Some(AdmissionScope::SpaceMutating) {
            continue;
        }
        match static_chord_ordinal(chord) {
            Some(10) => {
                return Err(format!(
                    "fixture-leg: chord '{chord}' names Desktop 10 (trailing creation is controller-owned); refusing"
                ));
            }
            Some(ordinal) => static_ordinals.push(ordinal as usize),
            // Relative/previous chords address no static ordinal: the
            // actuator gate still protects every dispatch at runtime.
            None => {}
        }
    }
    static_ordinals.sort_unstable();
    static_ordinals.dedup();
    for ordinal in &static_ordinals {
        if *ordinal <= baseline_count {
            return Err(format!(
                "fixture-leg: chord addresses user-Space ordinal {ordinal} (owned range starts at {}); refusing instead of using user Spaces",
                baseline_count + 1
            ));
        }
        if *ordinal > 9 {
            return Err(format!(
                "fixture-leg: chord ordinal {ordinal} is past Desktop-9 (unmappable); refusing"
            ));
        }
    }
    let max_ordinal = static_ordinals.into_iter().max();
    // Relative-only (or post-only, though this plans only space-mutating
    // legs) still retains a bounded pair of destinations for the latency
    // plan; static chords retain exactly up to their max ordinal.
    let top = max_ordinal.unwrap_or((baseline_count + 2).min(9));
    if top > 9 {
        return Err(format!(
            "fixture-leg: owned range would exceed Desktop-9 (baseline {baseline_count}); refusing"
        ));
    }
    if top <= baseline_count {
        return Err(format!(
            "fixture-leg: chord ordinals name only user Spaces (baseline {baseline_count}); refusing"
        ));
    }
    Ok((baseline_count + 1)..=top)
}

/// Fresh occupancy inventory for the post-only owned-Space path: every
/// non-fixture window whose membership includes a starting-display SID maps
/// to that SID's 1-based ordinal within the row, and `n` is the highest such
/// ordinal. Fixture-scope windows are auxiliary and never occupancy.
/// Unreadable non-fixture membership is never guessed at: `n` degrades to
/// the full row length (an explicit audit upper bound) and the unreadable
/// count is recorded. Read-only; IDs and ordinals only.
fn post_only_occupancy<'a>(
    spaces_before: &[u64],
    members: impl IntoIterator<Item = (&'a str, &'a Result<Vec<u64>, String>)>,
) -> PostOnlyOccupancy {
    let mut occupied = 0;
    let mut unreadable = 0;
    for (bundle, spaces) in members {
        if bundle == FIXTURE_SCOPE {
            continue;
        }
        match spaces {
            Ok(spaces) => {
                for sid in spaces {
                    if let Some(index) = spaces_before.iter().position(|candidate| candidate == sid)
                    {
                        occupied = occupied.max(index + 1);
                    }
                }
            }
            Err(_) => unreadable += 1,
        }
    }
    if unreadable > 0 {
        PostOnlyOccupancy {
            n: spaces_before.len(),
            n_kind: "upper_bound",
            n_unreadable_windows: unreadable,
        }
    } else {
        PostOnlyOccupancy {
            n: occupied,
            n_kind: "occupied",
            n_unreadable_windows: 0,
        }
    }
}

/// Plan the single post-only owned Space: exactly one append after the
/// fresh starting-display row, refused when that ordinal would leave the
/// Desktop-9 holder mapping (`spaces_before.len() >= 9`, the same ceiling
/// [`plan_owned_destinations`] applies to appended destinations; position
/// 10+ is unmappable by the send path).
fn post_only_owned_ordinal(spaces_before: &[u64]) -> Result<usize, String> {
    let ordinal = spaces_before.len() + 1;
    if ordinal > 9 {
        return Err(format!(
            "fixture-leg: {} user Spaces leave no representable post-only owned Space (holders map 1..=9); refusing instead of using user Spaces",
            spaces_before.len()
        ));
    }
    Ok(ordinal)
}

/// Fresh occupancy inventory result (see [`post_only_occupancy`]).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PostOnlyOccupancy {
    n: usize,
    n_kind: &'static str,
    n_unreadable_windows: usize,
}

/// Established post-only owned-Space evidence: the starting display, the
/// fresh occupancy inventory, the created SID at its append ordinal, the
/// recorded starting SID, and the row before/after. IDs and ordinals only,
/// no titles.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PostOnlyOwnedSpace {
    display: usize,
    n: usize,
    n_kind: &'static str,
    n_unreadable_windows: usize,
    sid: u64,
    ordinal: usize,
    starting_sid: u64,
    spaces_before: Vec<u64>,
    spaces_after: Vec<u64>,
}

impl PostOnlyOwnedSpace {
    fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "display": self.display,
            "n": self.n,
            "n_kind": self.n_kind,
            "n_unreadable_windows": self.n_unreadable_windows,
            "sid": self.sid,
            "ordinal": self.ordinal,
            "starting_sid": self.starting_sid,
            "spaces_before": self.spaces_before,
            "spaces_after": self.spaces_after,
        })
    }
}

/// Bounded harness pacing between the fixture-side focus and the synthetic
/// chord post (milliseconds): comfortably longer than the product's input
/// eligibility attest cadence (250ms) plus the observation publish/reconcile
/// that follows a foreground change, so the single owned post is dispatched
/// rather than consumed-refused. Purely harness-side; product policy is
/// unchanged.
const FOCUS_SETTLE_MS: u64 = 500;

/// Bounded settle for the post-only occupancy proof after the window join
/// (milliseconds): the join already proved CG existence, so only the SLS
/// membership read can lag.
const POST_ONLY_PLACEMENT_POLL_MS: u64 = 2_000;

/// One joined window's fresh membership read: the wid plus either readable
/// SIDs or the read failure (never a guess).
#[cfg(target_os = "macos")]
type WindowMembership = (u32, Result<Vec<u64>, String>);

/// One fresh membership read per joined window: `Ok(spaces)` when readable,
/// `Err` with the read failure otherwise. A window absent from the
/// inventory is unreadable, never assumed placed.
#[cfg(target_os = "macos")]
fn joined_window_memberships(wids: &[u32]) -> Vec<WindowMembership> {
    match crate::mc_observe::enumerate_window_members() {
        Ok(members) => {
            let mut by_wid: std::collections::HashMap<u32, Result<Vec<u64>, String>> = members
                .into_iter()
                .map(|member| (member.wid, member.spaces))
                .collect();
            wids.iter()
                .map(|wid| {
                    let spaces = by_wid.remove(wid).unwrap_or_else(|| {
                        Err("window absent from the fresh CG/SLS inventory".to_owned())
                    });
                    (*wid, spaces)
                })
                .collect()
        }
        Err(error) => wids
            .iter()
            .map(|wid| {
                (
                    *wid,
                    Err(format!("CG/SLS membership inventory unreadable ({error})")),
                )
            })
            .collect(),
    }
}

/// Bounded poll proving every joined window is readable on the owned Space.
/// Returns the last observed memberships when the deadline passes, so the
/// refusal names each wid and what was read (never a guess).
#[cfg(target_os = "macos")]
fn await_joined_on_owned_space(
    joined: &[(String, u32)],
    owned_sid: u64,
) -> Result<(), Vec<WindowMembership>> {
    let wids: Vec<u32> = joined.iter().map(|(_, wid)| *wid).collect();
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(POST_ONLY_PLACEMENT_POLL_MS);
    loop {
        let observed = joined_window_memberships(&wids);
        if observed.iter().all(|(_, spaces)| {
            spaces
                .as_ref()
                .is_ok_and(|spaces| spaces.contains(&owned_sid))
        }) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(observed);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Refusal body for the post-only placement checks: the established
/// owned-Space evidence plus one entry per joined window (wid with its
/// observed membership or read failure; no titles).
#[cfg(target_os = "macos")]
fn post_only_placement_refusal(
    evidence: &PostOnlyOwnedSpace,
    observed: &[WindowMembership],
    error: &str,
    current_owned_space: Option<bool>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "stage": "post-only-owned-space",
        "error": error,
        "owned_space": evidence.evidence(),
        "windows": observed
            .iter()
            .map(|(wid, spaces)| match spaces {
                Ok(spaces) => serde_json::json!({"wid": wid, "spaces": spaces}),
                Err(read_error) => serde_json::json!({"wid": wid, "membership_error": read_error}),
            })
            .collect::<Vec<_>>(),
    });
    if let Some(current) = current_owned_space {
        body["current_owned_space"] = current.into();
    }
    body
}

/// macOS-only leg body (see [`run_leg_body`]).
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)]
fn run_leg_body_macos(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureLegOpts,
) -> Result<LegReports, (i32, String)> {
    use crate::live_leg::PendingLaunch;
    // Owned private run directory first (before ANY fixture/product/Space
    // effect): a refusal aborts with zero effects.
    let run_dir = create_owned_run_dir("run").map_err(|e| (2, e))?;
    let product_dir = run_dir.join("product");
    std::fs::create_dir(&product_dir)
        .map_err(|e| (1, format!("fixture-leg: product dir refused ({e})")))?;
    // Pending fixture launch BEFORE `open`: any post-open/pre-bind failure
    // below holds residue until the identity joins or absence proves, and
    // finish can never report clean over it.
    leg.note_pending_launch(PendingLaunch {
        run_dir: run_dir.clone(),
        expected_bundle: FIXTURE_SCOPE.to_owned(),
        what: "fixture",
    });
    // Fixture launch through LaunchServices (exact fixture scope enforced
    // at bind, never by path trust).
    let bundle = fixture_bundle_path().map_err(|e| (2, e))?;
    if let Err(e) = launch_fixture_bundle(&bundle, &run_dir) {
        let _ = leg.resolve_pending_absence("fixture");
        return Err((1, e));
    }
    let announcement = read_announcement(&run_dir).map_err(|e| (1, e))?;
    let parsed = crate::fixture_post::parse_fixture_announcement(&announcement)
        .map_err(|e| (1, format!("fixture-leg: announcement refused: {e}")))?;
    // Identity before registration: exact fixture scope (direct-binary
    // launches carry no bundle identity and refuse here), readable
    // executable plus LaunchServices launch date.
    let identity = crate::fixture_post::fixture_process_identity(parsed.pid)
        .map_err(|e| (1, format!("fixture-leg: identity unreadable ({e})")))?;
    match identity.bundle.as_deref() {
        Some(bundle) if bundle == FIXTURE_SCOPE => {}
        _ => {
            return Err((
                1,
                "fixture-leg: live bundle is not the signed fixture scope; refusing".to_owned(),
            ));
        }
    }
    let (exe, launch) = match (identity.exe, identity.launch) {
        (Some(exe), Some(launch)) if !exe.is_empty() && !launch.is_empty() => (exe, launch),
        _ => {
            return Err((
                1,
                "fixture-leg: executable or launch date unreadable (no LaunchServices lifetime); refusing"
                    .to_owned(),
            ));
        }
    };
    // Nonce claim must equal the announcement (the only nonce source).
    let nonce = read_run_nonce(&run_dir).map_err(|e| (1, e))?;
    if nonce != parsed.nonce {
        return Err((
            1,
            "fixture-leg: nonce mismatch (stale or foreign run); refusing".to_owned(),
        ));
    }
    leg.register_fixture(FixtureRecord {
        pid: parsed.pid,
        exe,
        launch,
        socket_path: parsed.socket.clone(),
        run_dir: run_dir.clone(),
    })?;
    leg.transport_mut()
        .set_fixture_control(parsed.socket.clone(), run_dir.clone());
    // Owned windows: one caller per chord, spawned through the control
    // socket, then joined to native window IDs before anything posts.
    // Registration precedes action: unjoined callers refuse with no posts.
    let callers: Vec<String> = (0..opts.chords.len()).map(|i| format!("w{i}")).collect();
    let needs_probe = opts
        .chords
        .iter()
        .any(|chord| required_scope(chord) == Some(AdmissionScope::SpaceMutating));
    // Post-only legs driven by this controller run on one controller-owned
    // Space beyond the fresh occupancy inventory, so no chord can post on a
    // user Space. The return route is proven by execution first (it also
    // proves the teardown restore), then the fresh row/inventory, the single
    // append, and the switch. Mixed or space-mutating legs keep the existing
    // ordering below exactly.
    let post_only = !opts.chords.is_empty()
        && opts
            .chords
            .iter()
            .all(|chord| required_scope(chord) == Some(AdmissionScope::PostOnly));
    let mut owned_sids: Vec<u64> = Vec::new();
    let mut starting_sid: Option<u64> = None;
    let mut owned_space: Option<PostOnlyOwnedSpace> = None;
    if post_only {
        leg.prove_return()?;
        let before = leg.transport_mut().space_snapshot().map_err(|e| {
            (
                1,
                format!("fixture-leg: post-only starting snapshot unreadable ({e})"),
            )
        })?;
        let row = before.displays.first().ok_or_else(|| {
            (
                1,
                "fixture-leg: post-only starting snapshot has no display; refusing".to_owned(),
            )
        })?;
        starting_sid = Some(row.current);
        let spaces_before = row.spaces.clone();
        let ordinal = post_only_owned_ordinal(&spaces_before).map_err(|e| (2, e))?;
        // Fresh occupancy inventory, read-only: no synthetic posts, no
        // writes. Fixture auxiliary windows are excluded; unreadable
        // non-fixture membership degrades to the explicit row upper bound.
        let members = crate::mc_observe::enumerate_window_members().map_err(|e| {
            (
                1,
                format!("fixture-leg: post-only occupancy inventory unreadable ({e})"),
            )
        })?;
        let occupancy = post_only_occupancy(
            &spaces_before,
            members
                .iter()
                .map(|member| (member.bundle.as_str(), &member.spaces)),
        );
        let created = leg.transport_mut().space_create(0).map_err(|e| {
            (
                1,
                format!("fixture-leg: post-only owned Space creation refused ({e})"),
            )
        })?;
        let sid = created.sid;
        // Register before any later refusal so teardown removes exactly this
        // owned Space.
        leg.note_owned_space(0, sid);
        owned_sids.push(sid);
        if let Some(shortfall) = created.mc.shortfall_text("post-only owned Space create") {
            return Err((
                1,
                format!("fixture-leg: post-only owned Space creation: {shortfall}"),
            ));
        }
        leg.transport_mut().space_goto(0, sid).map_err(|e| {
            (
                1,
                format!("fixture-leg: post-only owned Space switch refused ({e})"),
            )
        })?;
        let after = leg.transport_mut().space_snapshot().map_err(|e| {
            (
                1,
                format!("fixture-leg: post-only owned Space switch unverified ({e})"),
            )
        })?;
        let after_row = after.displays.first().ok_or_else(|| {
            (
                1,
                "fixture-leg: post-only switch snapshot has no display; refusing".to_owned(),
            )
        })?;
        let evidence = PostOnlyOwnedSpace {
            display: 0,
            n: occupancy.n,
            n_kind: occupancy.n_kind,
            n_unreadable_windows: occupancy.n_unreadable_windows,
            sid,
            ordinal,
            starting_sid: row.current,
            spaces_before,
            spaces_after: after_row.spaces.clone(),
        };
        let observed_ordinal = after_row
            .spaces
            .iter()
            .position(|candidate| *candidate == sid)
            .map(|index| index + 1);
        // The switch must land on the owned SID at exactly one append past
        // the fresh row; anything else refuses (teardown restores/removes).
        if after_row.current != sid
            || observed_ordinal != Some(ordinal)
            || after_row.spaces.len() != evidence.spaces_before.len() + 1
        {
            return Err((
                1,
                serde_json::json!({
                    "stage": "post-only-owned-space",
                    "error": format!(
                        "post-only owned Space switch unverified (current {}, observed ordinal {:?}, expected ordinal {ordinal} after exactly one append)",
                        after_row.current, observed_ordinal
                    ),
                    "owned_space": evidence.evidence(),
                })
                .to_string(),
            ));
        }
        owned_space = Some(evidence);
    }
    spawn_owned_windows(&parsed.socket, &nonce, &callers).map_err(|e| (1, e))?;
    let joined =
        join_fixture_windows(parsed.pid, &parsed.socket, &nonce, &callers).map_err(|e| (1, e))?;
    for (caller, wid) in &joined {
        leg.note_owned_window(caller, parsed.pid, *wid);
    }
    // Post-only placement is proven before the product launches: every
    // joined window must be readable on the owned Space, so no later chord
    // can post on a user Space.
    if let Some(evidence) = &owned_space
        && let Err(observed) = await_joined_on_owned_space(&joined, evidence.sid)
    {
        return Err((
            1,
            post_only_placement_refusal(
                evidence,
                &observed,
                "post-only fixture windows are not readable on the owned Space before product launch; refusing without posting",
                None,
            )
            .to_string(),
        ));
    }
    // Executed return probe before any space-mutating chord, and before
    // the product starts (fewer moving parts during the proof; the holder
    // baseline captured at begin still precedes the Authentic ensure).
    // Retained owned destinations for space-mutating chords: after the
    // full preflight plus the successful independent probe, pre-append the
    // planned finite test Spaces and register each for owned-only removal
    // at finish (retained until after the fixture closes). The product
    // receives their exact SIDs through the fixture gate flags, so its
    // numbered selects/sends address appended ordinary positions with the
    // product chords (semantics preserved) while the actuator gate keeps
    // every dispatch off baseline SIDs even if the mapping moves.
    // Post-only legs already created and switched to their single owned
    // Space above and never enter this block.
    if needs_probe {
        leg.prove_return()?;
        let baseline = leg.baseline().clone();
        let first = baseline.native.displays.first().ok_or_else(|| {
            (
                1,
                "fixture-leg: baseline native row has no display; refusing".to_owned(),
            )
        })?;
        starting_sid = Some(first.current);
        let owned_range =
            plan_owned_destinations(&opts.chords, first.spaces.len()).map_err(|e| (2, e))?;
        for _ in owned_range {
            let created = leg.transport_mut().space_create(0).map_err(|e| {
                (
                    1,
                    format!("fixture-leg: owned destination creation refused ({e})"),
                )
            })?;
            // Register before any refusal so teardown removes exactly this
            // owned Space.
            leg.note_owned_space(0, created.sid);
            owned_sids.push(created.sid);
            if let Some(shortfall) = created.mc.shortfall_text("owned destination create") {
                return Err((
                    1,
                    format!("fixture-leg: owned destination creation: {shortfall}"),
                ));
            }
        }
    }
    // Pending product launch BEFORE `open`: wrapper exit is never native
    // success, so the leg holds this residue until the LaunchServices
    // identity (fresh PID plus run ID plus bundle/exe/launch join) binds.
    let run_id = format!(
        "fixture-leg-{}-{}",
        std::process::id(),
        leg.baseline()
            .native
            .displays
            .first()
            .map(|row| row.current)
            .unwrap_or(0)
    );
    std::fs::write(product_dir.join("run-id"), format!("{run_id}\n"))
        .map_err(|e| (1, format!("fixture-leg: product run-id refused ({e})")))?;
    leg.note_pending_launch(PendingLaunch {
        run_dir: product_dir.clone(),
        expected_bundle: crate::BUNDLE_ID.to_owned(),
        what: "product",
    });
    // Product launch through the signed bundle (LaunchServices
    // attribution for the TCC grant; never a direct executable spawn).
    // The leg seconds ride verbatim (always within the product bound by
    // the entry validation above: the exact flag is accepted).
    let product_bundle = product_bundle_path().map_err(|e| (2, e))?;
    let argv = product_argv(opts, &product_dir, &owned_sids, starting_sid);
    match launch_product_ls(
        &product_bundle,
        &product_dir,
        &argv,
        &run_id,
        opts.hover_probe,
    ) {
        Ok(product) => leg.register_product_ls(product)?,
        Err(e) => {
            let _ = leg.resolve_pending_absence("product");
            return Err((1, e));
        }
    }
    // One admission covering the widest chord scope, one driver bind
    // (single bind-time signature verification), then the repeated posts,
    // one chord per owned window in order.
    let widest = opts
        .chords
        .iter()
        .find(|chord| required_scope(chord) == Some(AdmissionScope::SpaceMutating));
    let admission = leg.admit(parsed.pid, widest.map(String::as_str))?;
    let live = crate::fixture_post::LiveTransport;
    let driver = crate::fixture_post::FixtureDriver::bind(
        &live,
        &announcement,
        &nonce,
        &opts.signer,
        admission,
    )?;
    // Operator settle (harness only): hold before the first chord and again
    // after the last, so an operator can drive extra owned fixture windows
    // and read native frames while the product runs. Zero when unset.
    let settle = settle_ms(opts.seconds).map_err(|e| (2, e))?;
    if settle > 0 {
        settle_hold(settle)?;
    }
    // Post-only safety gate: the owned Space must still be current and every
    // joined window still readable on it immediately before any chord posts.
    // Any drift refuses without posting (teardown restores/removes).
    if let Some(evidence) = &owned_space {
        let current_owned_space = leg
            .transport_mut()
            .space_snapshot()
            .map(|topology| {
                topology
                    .displays
                    .first()
                    .is_some_and(|row| row.current == evidence.sid)
            })
            .unwrap_or(false);
        let placement = await_joined_on_owned_space(&joined, evidence.sid);
        if !current_owned_space || placement.is_err() {
            let observed = placement.err().unwrap_or_default();
            return Err((
                1,
                post_only_placement_refusal(
                    evidence,
                    &observed,
                    "post-only owned-Space placement drifted before the chord posts; refusing without posting",
                    Some(current_owned_space),
                )
                .to_string(),
            ));
        }
    }
    let mut reports = Vec::new();
    for (chord, (caller, wid)) in opts.chords.iter().zip(joined.iter()) {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err((
                1,
                "fixture-leg: interrupted (SIGINT/SIGTERM latched); restoring".to_owned(),
            ));
        }
        // Each chord is driven to its OWN window: focus it through the
        // fixture's own socket first (the post path refuses a target that is
        // not the app's selected window), then post.
        driver.focus(caller, *wid)?;
        // Harness pacing, not product policy: the product consumes owned
        // chords before dispatch and its input owner refuses a basic hit
        // until the eligibility facts attest the fresh focus (250ms attest
        // cadence plus the observation publish/reconcile that follows a
        // foreground change). A post issued while focus is still settling is
        // consumed-refused-basic and never reaches the core, so keep one
        // bounded margin here; the settle polls the interrupt latch.
        settle_hold(FOCUS_SETTLE_MS)?;
        let report: serde_json::Value =
            serde_json::from_str(&driver.post(caller, *wid, chord, opts.modifier)?)
                .map_err(|e| (1, e.to_string()))?;
        reports.push(report);
    }
    if settle > 0 {
        settle_hold(settle)?;
    }
    // Opt-in native hover proof, after every chord and the final operator
    // settle so its own bounded pointer/focus sequence is the last effect
    // before teardown. Any failure keeps the completed chord audits in the
    // refusal envelope (a layout mismatch names the observed geometry).
    let hover_probe = if opts.hover_probe {
        match crate::fixture_hover::run(
            &driver,
            parsed.pid,
            &joined,
            opts.modifier,
            &product_dir.join("stderr.log"),
        ) {
            Ok(value) => Some(value),
            Err((code, error)) => {
                return Err((
                    code,
                    serde_json::json!({
                        "stage": "hover-probe",
                        "error": error,
                        "chords": reports,
                        "hover_probe": {"outcome": "refused"},
                    })
                    .to_string(),
                ));
            }
        }
    } else {
        None
    };
    Ok(LegReports {
        chords: reports,
        hover_probe,
        owned_space: owned_space.map(|evidence| evidence.evidence()),
    })
}

/// Spawn the leg's caller windows through the owned control socket. The
/// reply must carry a passing entry per caller; anything else refuses
/// before any join or post.
#[cfg(target_os = "macos")]
fn spawn_owned_windows(socket: &str, nonce: &str, callers: &[String]) -> Result<(), String> {
    let ids: Vec<&str> = callers.iter().map(String::as_str).collect();
    let reply = crate::fixture_post::send_fixture_spawn(socket, nonce, &ids)
        .map_err(|e| format!("fixture-leg: window spawn refused ({e})"))?;
    let windows = reply
        .get("result")
        .and_then(|result| result.get("windows"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "fixture-leg: spawn reply lacks window results".to_owned())?;
    for caller in callers {
        let ok = windows.iter().any(|entry| {
            entry.get("id").and_then(serde_json::Value::as_str) == Some(caller.as_str())
                && entry.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
        });
        if !ok {
            return Err(format!(
                "fixture-leg: server refused to spawn caller window {caller}"
            ));
        }
    }
    Ok(())
}

/// Join every caller to its live native window ID through AX identity.
/// The fixture bundle is AX-trusted (`preflight-app` proves it), so the
/// join is: announced fixture PID -> its own `AXWindows` -> the
/// server-controlled title parsed for caller+run tag -> AX window id,
/// corroborated by a fresh title-free CG (pid, wid) entry. CG titles need
/// Screen Recording, which this leg never uses, and neither a title nor a
/// PID is trusted alone: the AX application element scopes the read to the
/// announced fixture process, and the CG basic join proves the same wid is
/// a live window of that PID. Unjoined callers refuse before any post;
/// pre-existing foreign windows never match (caller/run-tag join).
#[cfg(target_os = "macos")]
fn join_fixture_windows(
    pid: u32,
    socket: &str,
    nonce: &str,
    callers: &[String],
) -> Result<Vec<(String, u32)>, String> {
    let app = crate::ffi::AxElement::application(pid as i32)
        .ok_or_else(|| format!("fixture-leg: no AX application element for pid {pid}"))?;
    let elements = app
        .windows()
        .map_err(|code| format!("fixture-leg: AXWindows unreadable for pid {pid} ({code})"))?;
    let cg = crate::ffi::cg_window_list_basic()
        .map_err(|e| format!("fixture-leg: CG snapshot unreadable ({e})"))?;
    let mut joined = Vec::new();
    for caller in callers {
        let snapshot = crate::fixture_post::socket_snapshot_for(socket, nonce, caller)
            .map_err(|e| format!("fixture-leg: caller {caller} snapshot refused ({e})"))?;
        let mut wid = None;
        for element in &elements {
            let Ok(Some(title)) = crate::ffi::ax_window_title(element) else {
                continue;
            };
            let Ok((native_caller, native_tag)) = crate::fixture_post::parse_fixture_title(&title)
            else {
                continue;
            };
            if native_caller != *caller || native_tag != snapshot.run_tag {
                continue;
            }
            let Some(window) = crate::ffi::ax_window_id(element) else {
                continue;
            };
            // Independent corroboration: the same window id must be a live
            // CG window owned by the announced fixture PID.
            if !cg
                .iter()
                .any(|entry| entry.pid == pid && entry.wid == window)
            {
                continue;
            }
            wid = Some(window);
            break;
        }
        match wid {
            Some(wid) => joined.push((caller.clone(), wid)),
            None => {
                return Err(format!(
                    "fixture-leg: caller {caller} has no live AX window join; refusing"
                ));
            }
        }
    }
    Ok(joined)
}

/// Read the run nonce from the owned announcement file (the only nonce
/// source; never logged).
fn read_run_nonce(run_dir: &std::path::Path) -> Result<String, String> {
    let path = run_dir.join("announcement.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("fixture-leg: announcement file unreadable ({e})"))?;
    let value: serde_json::Value = serde_json::from_str(raw.trim())
        .map_err(|_| "fixture-leg: announcement is not JSON".to_owned())?;
    match value.get("nonce").and_then(serde_json::Value::as_str) {
        Some(nonce) if nonce.len() == 64 && nonce.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(nonce.to_owned())
        }
        _ => Err("fixture-leg: announcement nonce malformed".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_opts(chords: Vec<&str>) -> FixtureLegOpts {
        FixtureLegOpts {
            seconds: 90,
            chords: chords.into_iter().map(str::to_owned).collect(),
            preset: InputPreset::Compatible,
            modifier: ProbeModifier::Ctrl,
            signer: "designated => test".to_owned(),
            focus_follows_cursor: false,
            hover_probe: false,
        }
    }

    #[test]
    fn leg_bounds_reject_before_effects() {
        // Usage shapes refuse purely: no baseline, no run dir, no launch.
        // The leg max reuses the product bound exactly (no hardcoded
        // mismatch): the seconds ride verbatim into the product flag.
        assert_eq!(LEG_MAX_SECONDS, crate::tile::MAX_TILE_SECONDS);
        assert_eq!(LEG_MIN_SECONDS, 60);
        let mut bad_seconds = test_opts(vec!["focus-h"]);
        bad_seconds.seconds = 10;
        assert!(run_fixture_leg(bad_seconds).unwrap_err().0 == 2);
        let mut over_seconds = test_opts(vec!["focus-h"]);
        over_seconds.seconds = LEG_MAX_SECONDS + 1;
        let err = run_fixture_leg(over_seconds).unwrap_err();
        assert_eq!(err.0, 2);
        let no_chords = test_opts(vec![]);
        assert!(run_fixture_leg(no_chords).unwrap_err().0 == 2);
        let bad_chord = test_opts(vec!["teleport"]);
        let err = run_fixture_leg(bad_chord).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("teleport"));
        // The hover probe needs one tall anchor plus a stacked pair: fewer
        // chords refuse with usage (exit 2) before any run dir or effect.
        let mut short_hover = test_opts(vec!["focus-h", "focus-j"]);
        short_hover.hover_probe = true;
        let err = run_fixture_leg(short_hover).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("--hover-probe"), "{}", err.1);
    }

    #[test]
    fn chord_post_waits_for_focus_eligibility() {
        // The product consumes owned chords before dispatch and refuses a
        // basic hit until its eligibility attest catches the fresh focus, so
        // the runner must pace between the fixture-side focus and the post.
        let source = include_str!("fixture_leg.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let focus_at = production
            .find("driver.focus(caller, *wid)?;")
            .expect("focus");
        let pace_at = production
            .find("settle_hold(FOCUS_SETTLE_MS)?;")
            .expect("focus pacing");
        let post_at = production
            .find("driver.post(caller, *wid, chord")
            .expect("post");
        assert!(focus_at < pace_at && pace_at < post_at);
        // 500ms constant is documented above the runner's constant; the
        // ordering is the code fact this test pins.
    }

    #[test]
    fn appkit_singleton_precedes_the_baseline() {
        // `NSScreen.screensHaveSeparateSpaces` answers with a stale `false`
        // until AppKit's shared application exists; a baseline captured
        // before that stores an untruthful preimage and the unchanged
        // Mission Control setting then compares unequal at teardown. Scoped
        // to the production section so this test's own literals cannot
        // self-match.
        let source = include_str!("fixture_leg.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let init_at = production
            .find("ensure_appkit_singleton();")
            .expect("fixture leg initializes the AppKit singleton");
        let begin_at = production
            .find("LiveLeg::begin(")
            .expect("fixture leg captures its baseline");
        assert!(
            init_at < begin_at,
            "AppKit singleton must exist before the baseline preimage"
        );
    }

    #[test]
    fn settle_window_is_bounded_and_lifetime_fitted() {
        // Unset/empty/zero: no hold at all (unchanged leg timing).
        assert_eq!(settle_ms_from(None, 90).expect("unset"), 0);
        assert_eq!(settle_ms_from(Some(""), 90).expect("empty"), 0);
        assert_eq!(settle_ms_from(Some("  "), 90).expect("blank"), 0);
        assert_eq!(settle_ms_from(Some("0"), 90).expect("zero"), 0);
        // Valid windows parse exactly (trimmed).
        assert_eq!(settle_ms_from(Some(" 15000 "), 90).expect("valid"), 15_000);
        assert_eq!(settle_ms_from(Some("20000"), 90).expect("max"), 20_000);
        // Non-numeric and over-bound values refuse before any effect.
        assert!(settle_ms_from(Some("1s"), 90).is_err());
        assert!(settle_ms_from(Some("-5"), 90).is_err());
        assert!(settle_ms_from(Some("20001"), 120).is_err());
        // Both holds plus the shutdown margin must fit the product lifetime.
        assert!(settle_ms_from(Some("20000"), 49).is_err());
        assert_eq!(settle_ms_from(Some("20000"), 50).expect("fits"), 20_000);
        assert!(settle_ms_from(Some("15000"), 39).is_err());
        assert_eq!(settle_ms_from(Some("15000"), 40).expect("fits"), 15_000);
    }

    #[test]
    fn product_argv_carries_fixed_scope_and_private_gate_ids() {
        // Production launch args: the fixed fixture scope, the leg seconds
        // verbatim (always product-accepted), the exact-run directory, and
        // the private gate IDs. Never unscoped, never another scope.
        let opts = test_opts(vec!["select-3", "send-3"]);
        let product_dir = std::path::Path::new("/tmp/leg-run/product");
        let argv = product_argv(&opts, product_dir, &[1001, 1002], Some(11));
        assert_eq!(argv[1], "tile");
        assert!(argv.contains(&"--user-start".to_owned()));
        let scopes: Vec<&String> = argv
            .windows(2)
            .filter(|pair| pair[0] == "--scope")
            .map(|pair| &pair[1])
            .collect();
        assert_eq!(scopes, vec![FIXTURE_SCOPE]);
        assert!(!argv.iter().any(|arg| arg == "--unscoped"));
        let seconds = argv
            .windows(2)
            .find(|pair| pair[0] == "--seconds")
            .map(|pair| pair[1].clone())
            .expect("seconds flag");
        assert_eq!(seconds, opts.seconds.to_string());
        assert!(seconds.parse::<u64>().expect("seconds") <= crate::tile::MAX_TILE_SECONDS);
        let run_dir = argv
            .windows(2)
            .find(|pair| pair[0] == "--run-dir")
            .map(|pair| pair[1].clone())
            .expect("run-dir flag");
        assert_eq!(run_dir, "/tmp/leg-run/product");
        let owned: Vec<u64> = argv
            .windows(2)
            .filter(|pair| pair[0] == "--fixture-owned-space")
            .map(|pair| pair[1].parse().expect("owned SID"))
            .collect();
        assert_eq!(owned, vec![1001, 1002]);
        let starting = argv
            .windows(2)
            .find(|pair| pair[0] == "--fixture-starting-space")
            .map(|pair| pair[1].parse().expect("starting SID"));
        assert_eq!(starting, Some(11));
        // Post-only legs driven by this controller now carry the single
        // created owned SID plus the recorded starting SID, arming the
        // existing product gate (no other argv change).
        let post = test_opts(vec!["focus-h"]);
        let argv = product_argv(&post, product_dir, &[2001], Some(11));
        let owned: Vec<u64> = argv
            .windows(2)
            .filter(|pair| pair[0] == "--fixture-owned-space")
            .map(|pair| pair[1].parse().expect("owned SID"))
            .collect();
        assert_eq!(owned, vec![2001]);
        let starting = argv
            .windows(2)
            .find(|pair| pair[0] == "--fixture-starting-space")
            .map(|pair| pair[1].parse().expect("starting SID"));
        assert_eq!(starting, Some(11));
        assert!(!argv.iter().any(|arg| arg == "--focus-follows-cursor"));
        let mut ffc = test_opts(vec!["focus-h"]);
        ffc.focus_follows_cursor = true;
        let argv = product_argv(&ffc, product_dir, &[], None);
        assert_eq!(
            argv.iter()
                .filter(|arg| *arg == "--focus-follows-cursor")
                .count(),
            1
        );
    }

    #[test]
    fn owned_destination_planning_never_targets_user_spaces() {
        let chords = |names: Vec<&str>| names.into_iter().map(str::to_owned).collect::<Vec<_>>();
        // Baseline of two: static chords at 3..=4 retain exactly those.
        assert_eq!(
            plan_owned_destinations(&chords(vec!["select-3", "send-4"]), 2).expect("plans"),
            3..=4
        );
        // A chord naming a user-Space ordinal refuses planning (instead of
        // using user Spaces at runtime).
        assert!(plan_owned_destinations(&chords(vec!["select-1"]), 2).is_err());
        assert!(plan_owned_destinations(&chords(vec!["send-2"]), 2).is_err());
        // Nine user Spaces leave no representable ordinary target.
        assert!(plan_owned_destinations(&chords(vec!["rel-next"]), 9).is_err());
        assert!(plan_owned_destinations(&chords(vec!["select-3"]), 9).is_err());
        // Desktop-10 (trailing) is controller-owned creation: refused.
        assert!(plan_owned_destinations(&chords(vec!["select-0"]), 2).is_err());
        assert!(plan_owned_destinations(&chords(vec!["send-0"]), 2).is_err());
        // Relative-only legs still retain a bounded pair (actuator-gated).
        assert_eq!(
            plan_owned_destinations(&chords(vec!["rel-prev", "rel-next"]), 2).expect("plans"),
            3..=4
        );
        // Static ordinals must fit Desktop-9.
        assert!(plan_owned_destinations(&chords(vec!["select-3"]), 8).is_err());
        // Empty baseline refuses.
        assert!(plan_owned_destinations(&chords(vec!["select-3"]), 0).is_err());
        // Static ordinal mapping: digits map 1:1, trailing 0 is Desktop 10.
        assert_eq!(static_chord_ordinal("select-3"), Some(3));
        assert_eq!(static_chord_ordinal("send-1"), Some(1));
        assert_eq!(static_chord_ordinal("select-0"), Some(10));
        assert_eq!(static_chord_ordinal("rel-prev"), None);
        assert_eq!(static_chord_ordinal("focus-h"), None);
    }

    #[test]
    fn post_only_occupancy_excludes_fixture_windows_and_bounds_unreadable() {
        let spaces_before = [10, 11, 12];
        let readable = |sids: &[u64]| Ok::<Vec<u64>, String>(sids.to_vec());
        let unreadable = Err::<Vec<u64>, String>("SLS read refused".to_owned());
        // Windows map to the starting row's 1-based ordinal; the max is n.
        let occupancy = post_only_occupancy(
            &spaces_before,
            [
                ("com.apple.Safari", &readable(&[10])),
                ("com.apple.Terminal", &readable(&[11, 12])),
            ],
        );
        assert_eq!(
            occupancy,
            PostOnlyOccupancy {
                n: 3,
                n_kind: "occupied",
                n_unreadable_windows: 0,
            }
        );
        // A window on a SID outside the starting row is not occupancy.
        let occupancy =
            post_only_occupancy(&spaces_before, [("com.apple.Safari", &readable(&[99]))]);
        assert_eq!(occupancy.n, 0);
        // Fixture auxiliary windows never count as occupancy, even when
        // their membership read failed.
        let occupancy = post_only_occupancy(
            &spaces_before,
            [
                (FIXTURE_SCOPE, &unreadable),
                (FIXTURE_SCOPE, &readable(&[12])),
            ],
        );
        assert_eq!(
            occupancy,
            PostOnlyOccupancy {
                n: 0,
                n_kind: "occupied",
                n_unreadable_windows: 0,
            }
        );
        // Any unreadable non-fixture membership degrades to the explicit
        // row-length upper bound and is counted, never guessed at.
        let occupancy = post_only_occupancy(
            &spaces_before,
            [
                ("com.apple.Safari", &readable(&[10])),
                ("com.apple.Terminal", &unreadable),
            ],
        );
        assert_eq!(
            occupancy,
            PostOnlyOccupancy {
                n: 3,
                n_kind: "upper_bound",
                n_unreadable_windows: 1,
            }
        );
    }

    #[test]
    fn post_only_owned_ordinal_is_one_append_within_desktop_nine() {
        // Exactly one append past the fresh row.
        assert_eq!(post_only_owned_ordinal(&[]).expect("empty row"), 1);
        assert_eq!(post_only_owned_ordinal(&[10]).expect("one Space"), 2);
        assert_eq!(
            post_only_owned_ordinal(&[10, 11, 12, 13, 14, 15, 16, 17]).expect("eight Spaces"),
            9
        );
        // Position 10+ is unmappable by the holders: the same ceiling
        // plan_owned_destinations applies to appended destinations.
        let nine = [10, 11, 12, 13, 14, 15, 16, 17, 18];
        assert!(post_only_owned_ordinal(&nine).is_err());
        let no_chords: Vec<String> = Vec::new();
        assert!(plan_owned_destinations(&no_chords, 9).is_err());
    }

    #[test]
    fn completed_envelope_carries_established_owned_space() {
        let evidence = serde_json::json!({
            "display": 0,
            "n": 2,
            "n_kind": "occupied",
            "n_unreadable_windows": 0,
            "sid": 9001,
            "ordinal": 4,
            "starting_sid": 11,
            "spaces_before": [11, 12, 13],
            "spaces_after": [11, 12, 13, 9001],
        });
        let built = PostOnlyOwnedSpace {
            display: 0,
            n: 2,
            n_kind: "occupied",
            n_unreadable_windows: 0,
            sid: 9001,
            ordinal: 4,
            starting_sid: 11,
            spaces_before: vec![11, 12, 13],
            spaces_after: vec![11, 12, 13, 9001],
        };
        assert_eq!(built.evidence(), evidence);
        let envelope = completed_envelope(
            vec![serde_json::json!({"chord": "move-left"})],
            None,
            Some(evidence.clone()),
            serde_json::json!({"clean": true}),
        );
        assert_eq!(envelope["outcome"], "completed");
        assert_eq!(envelope["owned_space"], evidence);
        // No owned Space (space-mutating/mixed legs) leaves the key absent.
        let envelope = completed_envelope(vec![], None, None, serde_json::json!({"clean": true}));
        assert!(envelope.get("owned_space").is_none());
    }

    #[test]
    fn owned_run_dir_is_private_and_fresh() {
        let dir = create_owned_run_dir("test").expect("run dir creates");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        // A colliding leaf refuses rather than reusing.
        let dir2 = create_owned_run_dir("test").expect("unique leaf creates");
        assert_ne!(dir, dir2);
        let _ = std::fs::remove_dir(&dir);
        let _ = std::fs::remove_dir(&dir2);
    }

    #[test]
    fn scope_is_fixed_to_the_signed_fixture_bundle() {
        assert_eq!(FIXTURE_SCOPE, "io.github.omnitiler.live-fixture");
        assert_eq!(crate::fixture_post::FIXTURE_BUNDLE, FIXTURE_SCOPE);
    }

    static BUNDLE_TEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    fn bundle_test_parent(tag: &str) -> std::path::PathBuf {
        let id = BUNDLE_TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "fixture-leg-bundle-{tag}-{}-{id}",
            std::process::id()
        ))
    }

    fn write_test_file(path: &std::path::Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("test directories create");
        }
        std::fs::write(path, contents).expect("test file writes");
    }

    /// Assemble the signed sibling layout under `parent`: the probe bundle
    /// (the controller's own root) plus the fixture sibling, each with its
    /// owned marker plus executable. Returns the probe executable path.
    fn assemble_sibling_bundles(parent: &std::path::Path) -> std::path::PathBuf {
        let probe_root = parent.join(PRODUCT_BUNDLE_FILE_NAME);
        let probe_exe = probe_root.join(PRODUCT_EXE_REL);
        write_test_file(&probe_exe, "probe");
        write_test_file(
            &probe_root.join(PRODUCT_MARKER_REL),
            &format!("{}\n", crate::BUNDLE_ID),
        );
        let fixture_root = parent.join(FIXTURE_BUNDLE_FILE_NAME);
        write_test_file(&fixture_root.join(FIXTURE_EXE_REL), "fixture");
        write_test_file(
            &fixture_root.join(FIXTURE_MARKER_REL),
            &format!("{FIXTURE_SCOPE}\n"),
        );
        probe_exe
    }

    #[test]
    fn own_bundle_root_pins_app_contents_macos_shape() {
        use std::path::Path;
        assert_eq!(
            own_bundle_root(Path::new(
                "/p/OmniTilerProbe.app/Contents/MacOS/tiler-macos"
            )),
            Some(Path::new("/p/OmniTilerProbe.app").to_path_buf())
        );
        // Developer cargo layout is not a signed bundle layout.
        assert_eq!(
            own_bundle_root(Path::new("/p/target/debug/tiler-macos")),
            None
        );
        // Wrong leaf names refuse.
        assert_eq!(
            own_bundle_root(Path::new(
                "/p/OmniTilerProbe.app/Resources/MacOS/tiler-macos"
            )),
            None
        );
        assert_eq!(
            own_bundle_root(Path::new("/p/OmniTilerProbe.app/Contents/Bin/tiler-macos")),
            None
        );
        assert_eq!(
            own_bundle_root(Path::new(
                "/p/OmniTilerProbe.dir/Contents/MacOS/tiler-macos"
            )),
            None
        );
        assert_eq!(own_bundle_root(Path::new("/tiler-macos")), None);
    }

    #[test]
    fn bundle_paths_resolve_from_signed_exe_layout_regardless_of_cwd() {
        // The parent is a fresh temp directory, never the repository: the
        // resolvers take no working directory, so a LaunchServices launch
        // (whose CWD is not the repo) resolves the same signed siblings.
        let parent = bundle_test_parent("siblings");
        let exe = assemble_sibling_bundles(&parent);
        assert_eq!(
            resolve_product_bundle(&exe, None).expect("product resolves"),
            parent.join(PRODUCT_BUNDLE_FILE_NAME)
        );
        assert_eq!(
            resolve_fixture_bundle(&exe, None).expect("fixture resolves"),
            parent.join(FIXTURE_BUNDLE_FILE_NAME)
        );
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn bundle_paths_fail_closed_outside_signed_layout() {
        use std::path::Path;
        // Developer binary layout: no bundle root to anchor on, no
        // CWD guess. The refusal names the signed layout, never a
        // missing working directory.
        let exe = Path::new("/repo/target/debug/tiler-macos");
        let fixture_err = resolve_fixture_bundle(exe, None).expect_err("fixture refuses");
        assert!(
            fixture_err.contains("signed bundle layout"),
            "{fixture_err}"
        );
        assert!(!fixture_err.contains("working directory"), "{fixture_err}");
        let product_err = resolve_product_bundle(exe, None).expect_err("product refuses");
        assert!(
            product_err.contains("signed bundle layout"),
            "{product_err}"
        );
        assert!(!product_err.contains("working directory"), "{product_err}");
    }

    #[test]
    fn bundle_paths_refuse_missing_sibling_and_foreign_markers() {
        // Probe bundle only: the missing fixture sibling refuses with the
        // build-recipe guidance (never a bare guess elsewhere).
        let parent = bundle_test_parent("probe-only");
        let probe_root = parent.join(PRODUCT_BUNDLE_FILE_NAME);
        write_test_file(&probe_root.join(PRODUCT_EXE_REL), "probe");
        write_test_file(
            &probe_root.join(PRODUCT_MARKER_REL),
            &format!("{}\n", crate::BUNDLE_ID),
        );
        let exe = probe_root.join(PRODUCT_EXE_REL);
        let err = resolve_fixture_bundle(&exe, None).expect_err("missing sibling refuses");
        assert!(err.contains("fixture bundle missing"), "{err}");
        // Foreign product marker refuses before any effect.
        write_test_file(
            &probe_root.join(PRODUCT_MARKER_REL),
            "com.example.foreign\n",
        );
        let err = resolve_product_bundle(&exe, None).expect_err("foreign marker refuses");
        assert!(err.contains("marker mismatch"), "{err}");
        // Foreign fixture marker refuses the same way.
        let fixture_parent = bundle_test_parent("foreign-fixture");
        let foreign_exe = assemble_sibling_bundles(&fixture_parent);
        let foreign_root = fixture_parent.join(FIXTURE_BUNDLE_FILE_NAME);
        write_test_file(
            &foreign_root.join(FIXTURE_MARKER_REL),
            "com.example.foreign\n",
        );
        let err = resolve_fixture_bundle(&foreign_exe, None).expect_err("foreign fixture refuses");
        assert!(err.contains("marker mismatch"), "{err}");
        // A symlinked fixture bundle path refuses.
        let link_parent = bundle_test_parent("links");
        let _ = assemble_sibling_bundles(&link_parent);
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            link_parent.join(FIXTURE_BUNDLE_FILE_NAME),
            link_parent.join("FixtureLink.app"),
        )
        .expect("test symlink");
        let linked = link_parent.join("FixtureLink.app");
        let err = check_fixture_bundle(&linked).expect_err("symlinked fixture refuses");
        assert!(err.contains("symlink"), "{err}");
        let _ = std::fs::remove_dir_all(&parent);
        let _ = std::fs::remove_dir_all(&fixture_parent);
        let _ = std::fs::remove_dir_all(&link_parent);
    }

    #[test]
    fn bundle_overrides_keep_identity_checks() {
        let parent = bundle_test_parent("overrides");
        let exe = assemble_sibling_bundles(&parent);
        let probe_root = parent.join(PRODUCT_BUNDLE_FILE_NAME);
        let fixture_root = parent.join(FIXTURE_BUNDLE_FILE_NAME);
        // Empty override falls back to the executable-anchored default.
        assert_eq!(
            resolve_product_bundle(&exe, Some("")).expect("empty override falls back"),
            probe_root
        );
        assert_eq!(
            resolve_fixture_bundle(&exe, Some("")).expect("empty override falls back"),
            fixture_root
        );
        // Valid overrides resolve with the same checks applied.
        assert_eq!(
            resolve_product_bundle(&exe, Some(&probe_root.to_string_lossy()))
                .expect("valid product override"),
            probe_root
        );
        assert_eq!(
            resolve_fixture_bundle(&exe, Some(&fixture_root.to_string_lossy()))
                .expect("valid fixture override"),
            fixture_root
        );
        // Override to a marker-less directory refuses (product and fixture).
        let bare = parent.join("Bare.app");
        std::fs::create_dir_all(&bare).expect("bare dir creates");
        let err = resolve_product_bundle(&exe, Some(&bare.to_string_lossy()))
            .expect_err("marker-less product override refuses");
        assert!(err.contains("owned marker"), "{err}");
        let err = resolve_fixture_bundle(&exe, Some(&bare.to_string_lossy()))
            .expect_err("marker-less fixture override refuses");
        assert!(err.contains("owned marker"), "{err}");
        // Missing override path refuses with the build-recipe guidance.
        let missing = parent.join("Missing.app");
        let err = resolve_product_bundle(&exe, Some(&missing.to_string_lossy()))
            .expect_err("missing product override refuses");
        assert!(err.contains("product bundle missing"), "{err}");
        let _ = std::fs::remove_dir_all(&parent);
    }
}
