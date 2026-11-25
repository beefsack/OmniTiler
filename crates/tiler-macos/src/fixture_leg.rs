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
use crate::fixture_owned_space::{
    PostOnlyOwnedSpace, await_joined_on_owned_space, establish_post_only_owned_space,
    plan_owned_destinations, post_only_placement_refusal,
};
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

/// Diagnostic-only chord: one owned window receives two real `maximize-m`
/// posts through the product tap, with native AX/CG frames captured before,
/// between and after. The first press must move the frame; the second must
/// restore the exact base. This is native behaviour evidence, not slice-5
/// acceptance and not a product command.
pub const MAXIMIZE_TOGGLE_CHORD: &str = "maximize-toggle";

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
    /// Diagnostic-only: pre-seed the isolated intent store with malformed
    /// JSON before the product launches. The product must fail closed to an
    /// empty store and keep operating; never a product setting.
    pub corrupt_intent_store: bool,
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
pub(crate) fn settle_hold(ms: u64) -> Result<(), (i32, String)> {
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
    corrupt_store: Option<serde_json::Value>,
}

/// Completed success envelope: the chord reports plus any established
/// owned-Space or corrupt-store evidence. Pure over its inputs;
/// unit-tested.
fn completed_envelope(
    chords: Vec<serde_json::Value>,
    hover_probe: Option<serde_json::Value>,
    owned_space: Option<serde_json::Value>,
    corrupt_store: Option<serde_json::Value>,
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
    if let Some(store) = corrupt_store {
        envelope["corrupt_store"] = store;
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
                corrupt_store,
            } = reports;
            match leg.finish() {
                Ok(teardown) => {
                    let envelope = completed_envelope(
                        chords,
                        hover_probe,
                        owned_space,
                        corrupt_store,
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

/// Bounded harness pacing between the fixture-side focus and the synthetic
/// chord post (milliseconds): comfortably longer than the product's input
/// eligibility attest cadence (250ms) plus the observation publish/reconcile
/// that follows a foreground change, so the single owned post is dispatched
/// rather than consumed-refused. Purely harness-side; product policy is
/// unchanged.
const FOCUS_SETTLE_MS: u64 = 500;

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
    if opts.corrupt_intent_store {
        crate::fixture_corrupt_store::pre_seed(&product_dir)?;
    }
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
        let established = establish_post_only_owned_space(leg)?;
        owned_sids.push(established.sid);
        starting_sid = Some(established.starting_sid);
        owned_space = Some(established.evidence);
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
        && let Err(observed) = await_joined_on_owned_space(&joined, evidence.sid())
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
                    .is_some_and(|row| row.current == evidence.sid())
            })
            .unwrap_or(false);
        let placement = await_joined_on_owned_space(&joined, evidence.sid());
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
        let report: serde_json::Value = if chord == MAXIMIZE_TOGGLE_CHORD {
            crate::fixture_maximize::run(&driver, parsed.pid, *wid, caller, chord, opts.modifier)?
        } else {
            serde_json::from_str(&driver.post(caller, *wid, chord, opts.modifier)?)
                .map_err(|e| (1, e.to_string()))?
        };
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
    let corrupt_store = if opts.corrupt_intent_store {
        Some(crate::fixture_corrupt_store::capture(&product_dir)?)
    } else {
        None
    };
    Ok(LegReports {
        chords: reports,
        hover_probe,
        owned_space: owned_space.map(|evidence| evidence.evidence()),
        corrupt_store,
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
#[path = "fixture_leg_tests.rs"]
mod tests;
