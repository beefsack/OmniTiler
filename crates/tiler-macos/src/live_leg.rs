#![forbid(unsafe_code)]

//! Harness-enforced live leg controller for macOS fixture legs.
//!
//! A leg captures the full [`LiveBaseline`](crate::live_baseline::LiveBaseline)
//! before any input, persists the receipt atomically, proves the independent
//! return route by execution, admits commands only through unforgeable
//! single-leg admissions, and on every end path runs the full teardown
//! pipeline: stop the exact primary product first (proving the return works
//! after the primary exits), restore the starting Space, close exactly the
//! registered fixture windows, shut down and reap only owned resources,
//! remove only leg-created empty Spaces, stop the awake companion, and only
//! then capture and compare the teardown. Missing baselines, a failed return
//! proof, or any teardown mismatch aborts the leg; user state is never
//! automatically overwritten and cleanup is never skipped on errors.
//!
//! Admission scopes (checked by the fixture driver on every bind, post, and
//! focus): [`AdmissionScope::PostOnly`] for directional focus/move chords and
//! fixture focus, which never change Space membership;
//! [`AdmissionScope::SpaceMutating`] for numbered selects, send-follows, and
//! the bound relative/previous selects, which additionally require the
//! executed return probe ([`LiveLeg::prove_return`]) to have succeeded on a
//! newly created owned Space first.
//!
//! Admissions are single-leg and revocable: each carries the leg generation
//! plus a shared liveness token that `finish` and `Drop` revoke. A replayed
//! admission (another leg, another PID, another scope, or after finish)
//! refuses on every bind, post, and focus. A persisted recovery receipt is
//! never reusable as input authorization: only a live leg mints admissions.
//!
//! The return itself rides the existing owned public route only (see
//! [`crate::space_probe`]): one Mission Control goto press per display with
//! the exact-effect readback, zero window writes, zero synthetic posts, never
//! replayed. No new input route is introduced here. Evidence caveat: the
//! first integrated goto run failed on its immediate goto while the
//! corrected recovery+remove passed, so the initial live proof below is a
//! required gate, never a claimed result.
//!
//! Transports arrive through [`LegTransport`] so tests execute the real
//! orchestration (probe, restore, full teardown pipeline, admission
//! revocation) against an in-memory native table enforcing the same
//! invariants, plus real sockets, real child processes, and the real
//! filesystem, without touching the desktop.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::live_baseline::{
    BaselineMismatch, BaselineReads, LiveBaseline, capture_baseline, compare_teardown,
    persist_baseline_to, remove_baseline_receipt,
};
use crate::space_probe::{
    NativeSpaces, SpaceProbeProof, probe_return_to_starting, remove_owned_space,
    restore_starting_currents,
};

/// Bounded display-awake companion lifetime in seconds for one leg.
pub const LEG_CAFFEINATE_SECONDS: u64 = 300;

/// Bounded reap poll for owned non-child processes in milliseconds
/// (genuine deadline: the host offers no exit event for PIDs the leg did
/// not spawn as a `Child`; stated here where the timer is defined).
pub const REAP_POLL_MS: u64 = 5_000;

/// Residue artifact leaf written beside the receipt when a leg ends unclean
/// (finish failure or Drop backstop). Metadata only, never titles, paths of
/// user data, nonces, or requirement text.
pub const RESIDUE_FILE_NAME: &str = "live-leg-residue.json";

/// Process-wide leg generation: every leg mints a fresh one, so admissions
/// can never be replayed across legs even for the same fixture PID.
static LEG_GENERATION: AtomicU64 = AtomicU64::new(1);

/// What one admission authorizes. Space-mutating chords additionally require
/// the executed return probe; directional chords never change membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionScope {
    PostOnly,
    SpaceMutating,
}

impl AdmissionScope {
    /// True when an admission of `self` scope covers a chord needing
    /// `want`. Space-mutating admissions cover everything; post-only
    /// admissions never cover a Space change.
    #[must_use]
    pub const fn covers(self, want: Self) -> bool {
        match (self, want) {
            (Self::SpaceMutating, _) => true,
            (Self::PostOnly, Self::PostOnly) => true,
            (Self::PostOnly, Self::SpaceMutating) => false,
        }
    }
}

/// Required admission scope for one fixture chord name: directional
/// focus/move chords need post-only admission; numbered selects,
/// send-follows, and the bound relative/previous selects need
/// space-mutating admission. Unknown or stay chords have no scope and
/// refuse before any proof. Pure; unit-tested.
#[must_use]
pub fn required_scope(chord_name: &str) -> Option<AdmissionScope> {
    if crate::fixture_post::resolve_fixture_chord(chord_name).is_some() {
        if chord_name.starts_with("select-") || chord_name.starts_with("send-") {
            return Some(AdmissionScope::SpaceMutating);
        }
        return Some(AdmissionScope::PostOnly);
    }
    if crate::fixture_post::resolve_fixture_bound(chord_name).is_some() {
        return Some(AdmissionScope::SpaceMutating);
    }
    None
}

/// Unforgeable single-leg admission: bound to one fixture PID with one
/// scope, one leg generation, and a liveness token shared with its leg.
/// The only production constructor is [`LiveLeg::admit`]; fields are private
/// to this module so no caller can mint, widen, or replay an admission, and
/// a persisted receipt cannot be converted into one. `finish` and `Drop`
/// revoke the token, so use-after-finish refuses on every bind, post, and
/// focus. Cloning shares the token (same leg), never extends it.
#[derive(Debug, Clone)]
pub struct LegAdmission {
    fixture_pid: u32,
    scope: AdmissionScope,
    generation: u64,
    live: Rc<Cell<bool>>,
}

impl LegAdmission {
    fn seal(
        fixture_pid: u32,
        scope: AdmissionScope,
        generation: u64,
        live: Rc<Cell<bool>>,
    ) -> Self {
        Self {
            fixture_pid,
            scope,
            generation,
            live,
        }
    }

    /// The fixture PID this admission is bound to.
    #[must_use]
    pub const fn fixture_pid(self: &LegAdmission) -> u32 {
        self.fixture_pid
    }

    /// What this admission authorizes.
    #[must_use]
    pub const fn scope(self: &LegAdmission) -> AdmissionScope {
        self.scope
    }

    /// The leg generation that minted this admission.
    #[must_use]
    pub const fn generation(self: &LegAdmission) -> u64 {
        self.generation
    }

    /// True while the minting leg is still active (false after finish/Drop).
    #[must_use]
    pub fn is_live(self: &LegAdmission) -> bool {
        self.live.get()
    }

    /// Test-only revocation for the driver's replay proofs.
    #[cfg(test)]
    pub(crate) fn revoke_for_test(self: &LegAdmission) {
        self.live.set(false);
    }

    /// Test-only mint for the driver's scripted proofs. Production code has
    /// no equivalent: admissions arrive only through [`LiveLeg::admit`].
    #[cfg(test)]
    pub(crate) fn admission_for_test(fixture_pid: u32, scope: AdmissionScope) -> Self {
        Self::seal(fixture_pid, scope, 0, Rc::new(Cell::new(true)))
    }
}

/// One leg-created fixture window: the socket caller id plus the joined
/// native identity. Closes ride the caller id; teardown accounting rides
/// the native join.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedWindow {
    caller: String,
    pid: u32,
    wid: u32,
}
/// One LaunchServices-owned fixture instance, registered from birth: the
/// run nonce never leaves memory (never serialized, never logged), while
/// PID/executable/launch-date bind the lifetime and the socket/run-dir bind
/// the control channel. Shutdown re-verifies the identity before touching
/// the server, so PID reuse can never authorize a foreign process.
#[derive(Debug, Clone)]
pub struct FixtureRecord {
    pub pid: u32,
    pub exe: String,
    pub launch: String,
    pub socket_path: String,
    pub run_dir: PathBuf,
}

/// One leg-launched but not yet identity-bound run: registered BEFORE the
/// LaunchServices `open` so that any post-open/pre-bind failure (launch
/// spawn error, announcement timeout, bundle mismatch, unreadable
/// executable/launch date) still holds residue until the native identity
/// joins or absence is independently proven. Never killed on unknown
/// ownership: the residue names the retained run directory for audit.
#[derive(Debug, Clone)]
pub struct PendingLaunch {
    /// Owned run directory created before the launch (fixture run dir, or
    /// the `product` subdirectory for product launches).
    pub run_dir: PathBuf,
    /// Expected exact bundle scope (fixture or probe bundle ID).
    pub expected_bundle: String,
    /// What was launched (fixture or product) for the residue evidence.
    pub what: &'static str,
}

/// One leg-spawned product instance: the exact PID plus the executable path
/// and LaunchServices launch date observed at spawn plus the exact-run
/// directory carrying the stop file.
/// The `Child` itself is held by the leg for directly spawned helpers
/// (never a PID guess for signals); LaunchServices-launched products carry
/// no `Child` (the wrapper owns no product lifetime) and stop through the
/// exact-run stop file plus identity-verified poll reaping instead.
#[derive(Debug, Clone)]
pub struct ProductRecord {
    pub pid: u32,
    pub exe: String,
    pub launch: String,
    pub run_dir: PathBuf,
    /// True for LaunchServices launches (no held `Child`): stop rides the
    /// stop file plus `outcome.json` terminal evidence plus exact-identity
    /// poll reaping. False keeps the exact held-`Child` path.
    pub launch_services: bool,
}

/// Executed return proof: the Space-probe evidence plus the closing Mission
/// Control presence. Space IDs only; no titles, paths, or content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnProof {
    pub probe: SpaceProbeProof,
    pub mc_open_after: Option<bool>,
}

/// Terminal teardown report: restoration evidence plus residue. IDs and
/// counts only; never titles, paths, nonces, or requirement text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeardownReport {
    pub generation: u64,
    pub product_reaped: bool,
    pub current_restored: bool,
    pub restored: Vec<(usize, u64)>,
    pub mismatches: Vec<BaselineMismatch>,
    pub residue: Vec<String>,
    pub caffeinate_reaped: bool,
    pub owned_reaped: Vec<u32>,
    pub receipt_retained: bool,
}

/// Injected leg boundary. Baseline reads ride [`BaselineReads`], Space
/// mechanics ride [`NativeSpaces`]; the remaining methods are the leg's
/// live effects, each executed for real in production and scripted or
/// doubled with real sockets/processes in tests:
/// - `return_readiness` proves the return route is available with read-only
///   checks only (gates, SLS readability, exact-single Dock, currents still
///   at their starting values): no presses, no posts, no MC automation;
/// - `close_fixture_window` closes exactly one registered run-owned fixture
///   window over its control socket;
/// - `shutdown_fixture` re-verifies the registered executable/launch-date
///   identity, sends the nonce-gated shutdown, verifies the reap under a
///   genuine deadline, and removes only the owned run directory (never a
///   signal to a drifted PID);
/// - `process_gone` proves one exact PID exited (unreadable refuses);
/// - `awake_start`/`awake_stop` own the bounded exact awake child.
pub trait LegTransport: BaselineReads + NativeSpaces {
    fn return_readiness(&mut self, starting: &LiveBaseline) -> Result<(), String>;
    /// Close exactly one registered run-owned fixture window by its caller
    /// id over the leg-owned control socket. PID/WID ride along for the
    /// residue evidence only.
    fn close_fixture_window(&mut self, caller: &str, pid: u32, wid: u32) -> Result<(), String>;
    fn shutdown_fixture(&mut self, record: &FixtureRecord) -> Result<(), String>;
    fn process_gone(&mut self, pid: u32) -> Result<bool, String>;
    fn awake_start(&mut self, seconds: u64) -> Result<(), String>;
    fn awake_stop(&mut self) -> Result<bool, String>;
    fn receipt_dir(&mut self) -> Result<PathBuf, String>;
}

/// Production transport: real OS reads, the existing owned public Space
/// route, real fixture socket control, and the exact awake child. Never
/// constructed in tests.
pub struct LiveLegTransport {
    caffeinate: Option<crate::mc_lifecycle::live::CaffeinateGuard>,
    spaces: crate::space_probe::LiveNativeSpaces,
    control: Option<FixtureControl>,
}

/// Leg-owned fixture control channel: the socket path plus the owned run
/// directory carrying the announcement (the only nonce source).
#[derive(Debug, Clone)]
struct FixtureControl {
    socket_path: String,
    run_dir: PathBuf,
}

impl LiveLegTransport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            caffeinate: None,
            spaces: crate::space_probe::LiveNativeSpaces::new(LEG_CAFFEINATE_SECONDS),
            control: None,
        }
    }

    /// Point the transport at the leg-owned fixture control channel after
    /// the controller registers the LaunchServices instance. Window closes
    /// refuse until this is set: no socket, no close.
    pub fn set_fixture_control(&mut self, socket_path: String, run_dir: PathBuf) {
        self.control = Some(FixtureControl {
            socket_path,
            run_dir,
        });
    }
}

impl Default for LiveLegTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl BaselineReads for LiveLegTransport {
    fn read_protected_windows(
        &mut self,
    ) -> Result<Vec<crate::live_baseline::ProtectedWindow>, String> {
        crate::live_baseline::LiveBaselineReads::new().read_protected_windows()
    }

    fn read_holders(&mut self) -> Result<Vec<crate::live_baseline::HolderBaseline>, String> {
        crate::live_baseline::LiveBaselineReads::new().read_holders()
    }

    fn read_accent(&mut self) -> Result<crate::live_baseline::AccentBaseline, String> {
        crate::live_baseline::LiveBaselineReads::new().read_accent()
    }

    fn read_processes(&mut self) -> Result<Vec<crate::live_baseline::ProcessBaseline>, String> {
        crate::live_baseline::LiveBaselineReads::new().read_processes()
    }

    fn read_native(&mut self) -> Result<crate::live_baseline::NativeBaseline, String> {
        crate::live_baseline::LiveBaselineReads::new().read_native()
    }
}

impl NativeSpaces for LiveLegTransport {
    fn space_snapshot(&mut self) -> Result<crate::space_probe::SpaceTopology, String> {
        self.spaces.space_snapshot()
    }

    fn space_create(&mut self, display: usize) -> Result<u64, String> {
        self.spaces.space_create(display)
    }

    fn space_goto(&mut self, display: usize, sid: u64) -> Result<(), String> {
        self.spaces.space_goto(display, sid)
    }

    fn space_remove(&mut self, sid: u64) -> Result<(), String> {
        self.spaces.space_remove(sid)
    }

    fn space_require_empty(&mut self, sid: u64) -> Result<(), String> {
        self.spaces.space_require_empty(sid)
    }

    fn mc_present(&mut self) -> Option<bool> {
        self.spaces.mc_present()
    }
}

/// Bounded reap poll for an owned non-child PID (genuine deadline: no exit
/// event exists for PIDs the leg did not spawn; stated here).
fn poll_process_gone(pid: u32) -> Result<bool, String> {
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("reap: pid {pid} out of range"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(REAP_POLL_MS);
    loop {
        if !crate::ffi::process_exists(pid_i32) {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

impl LegTransport for LiveLegTransport {
    fn return_readiness(&mut self, starting: &LiveBaseline) -> Result<(), String> {
        // Read-only availability proof for the return route: session gates,
        // a fresh SLS snapshot, the exact-single Dock identity, and currents
        // still at their starting values (user drift since the baseline
        // refuses instead of rebasing silently). No presses, no posts.
        crate::mc_lifecycle::live::action_gates()
            .map_err(|e| format!("return readiness: action gates refuse ({e})"))?;
        let snapshot = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| format!("return readiness: SLS snapshot unreadable ({e})"))?;
        let dock = crate::mc_lifecycle::live::dock_pid()
            .map_err(|e| format!("return readiness: Dock identity uncertain ({e})"))?;
        if dock <= 1 {
            return Err("return readiness: Dock PID reserved; refusing".to_owned());
        }
        for display in &starting.native.displays {
            match snapshot
                .displays
                .get(display.index)
                .and_then(|row| row.current)
            {
                Some(current) if current == display.current => {}
                Some(_) => {
                    return Err(format!(
                        "return readiness: display {} current changed since the baseline; refusing without a fresh baseline",
                        display.index
                    ));
                }
                None => {
                    return Err(format!(
                        "return readiness: display {} current unreadable or display gone since the baseline; refusing",
                        display.index
                    ));
                }
            }
        }
        Ok(())
    }

    fn close_fixture_window(&mut self, caller: &str, pid: u32, wid: u32) -> Result<(), String> {
        let control = self.control.clone().ok_or_else(|| {
            "close: no leg-owned fixture control channel; refusing rather than guessing a close"
                .to_owned()
        })?;
        let nonce = read_nonce_from_run_dir(&control.run_dir)?;
        let reply =
            crate::fixture_post::send_fixture_close(&control.socket_path, &nonce, &[caller])
                .map_err(|e| format!("close: control exchange refused ({e})"))?;
        let ok = reply
            .get("result")
            .and_then(|result| result.get("windows"))
            .and_then(serde_json::Value::as_array)
            .map(|windows| {
                windows.iter().any(|entry| {
                    entry.get("id").and_then(serde_json::Value::as_str) == Some(caller)
                        && entry.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
                })
            })
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err(format!(
                "close: server refused the close of owned window pid {pid} wid {wid}"
            ))
        }
    }

    fn shutdown_fixture(&mut self, record: &FixtureRecord) -> Result<(), String> {
        // Identity first: a relaunched or PID-reused process never receives
        // our shutdown (which would authorize a foreign server), and no
        // signal is ever sent to a drifted PID.
        let live = crate::ffi::ns_app_identity_for_pid(i32::try_from(record.pid).unwrap_or(0));
        match (live.exe, live.launch_date) {
            (Some(exe), Some(launch)) if exe == record.exe && launch == record.launch => {}
            _ => {
                return Err(format!(
                    "shutdown: fixture pid {} identity drift (relaunch or PID reuse); refusing shutdown and signals; residue: foreign process untouched",
                    record.pid
                ));
            }
        }
        let nonce = read_nonce_from_run_dir(&record.run_dir)?;
        crate::fixture_post::send_fixture_shutdown(&record.socket_path, &nonce)
            .map_err(|e| format!("shutdown: control exchange refused ({e})"))?;
        match poll_process_gone(record.pid)? {
            true => {}
            false => {
                return Err(format!(
                    "shutdown: fixture pid {} still present past the reap deadline; residue: process not reaped (never signalled: not our child)",
                    record.pid
                ));
            }
        }
        remove_owned_run_dir(&record.run_dir)
    }

    fn process_gone(&mut self, pid: u32) -> Result<bool, String> {
        let pid_i32 = i32::try_from(pid).map_err(|_| format!("reap: pid {pid} out of range"))?;
        Ok(!crate::ffi::process_exists(pid_i32))
    }

    fn awake_start(&mut self, seconds: u64) -> Result<(), String> {
        if self.caffeinate.is_some() {
            return Err("awake: companion already held; refusing a second child".to_owned());
        }
        self.caffeinate = Some(
            crate::mc_lifecycle::live::CaffeinateGuard::spawn(seconds)
                .map_err(|e| format!("awake: caffeinate unspawnable ({e})"))?,
        );
        Ok(())
    }

    fn awake_stop(&mut self) -> Result<bool, String> {
        match self.caffeinate.as_mut() {
            Some(guard) => Ok(guard.stop()),
            None => Ok(true),
        }
    }

    fn receipt_dir(&mut self) -> Result<PathBuf, String> {
        crate::desktop_shortcuts::ensure_private_recovery_dir()
    }
}

/// Read the run nonce back from the owned announcement file (the only nonce
/// source; never logged, never serialized into reports).
fn read_nonce_from_run_dir(run_dir: &Path) -> Result<String, String> {
    let path = run_dir.join("announcement.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("shutdown: announcement file unreadable ({e})"))?;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|_| "shutdown: announcement is not JSON".to_owned())?;
        if let Some(nonce) = value.get("nonce").and_then(serde_json::Value::as_str)
            && nonce.len() == 64
            && nonce.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Ok(nonce.to_owned());
        }
    }
    Err("shutdown: no parsable nonce in the announcement file".to_owned())
}

/// Remove only the owned fixture run directory's known files plus the
/// directory itself. Known leaves are the fixture announcement/log/socket
/// plus the leg-created `product` subdirectory's known product leaves
/// (`stop-requested`, `outcome.json`, `.outcome.json.tmp`, `stdout.log`,
/// `stderr.log`, `run-id`). Every removal is symlink-guarded; an unknown
/// leaf is residue (refused here, never broad-deleted) so the next leg's
/// begin is blocked until inspection instead of silently dropping state.
/// Never recursive beyond the owned leaves, never a pre-existing directory.
fn remove_owned_run_dir(run_dir: &Path) -> Result<(), String> {
    for leaf in ["announcement.json", "stderr.log", "fixture.sock"] {
        remove_known_leaf(run_dir, leaf)?;
    }
    let product_dir = run_dir.join("product");
    match std::fs::symlink_metadata(&product_dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("shutdown: owned product dir is a symlink; refusing".to_owned());
        }
        Ok(meta) if meta.is_dir() => {
            for leaf in [
                "stop-requested",
                "outcome.json",
                ".outcome.json.tmp",
                "stdout.log",
                "stderr.log",
                "run-id",
            ] {
                remove_known_leaf(&product_dir, leaf)?;
            }
            // Unknown product leaves are residue, never broad-deleted: the
            // removal refuses so the receipt is retained for audit.
            let mut unknown = Vec::new();
            match std::fs::read_dir(&product_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        unknown.push(entry.file_name().to_string_lossy().into_owned());
                    }
                }
                Err(e) => {
                    return Err(format!("shutdown: owned product dir unreadable ({e})"));
                }
            }
            if !unknown.is_empty() {
                return Err(format!(
                    "shutdown: owned product dir holds unknown leaves ({}); residue retained, never broad-deleted",
                    unknown.join(",")
                ));
            }
            std::fs::remove_dir(&product_dir)
                .map_err(|e| format!("shutdown: owned product dir removal refused ({e})"))?;
        }
        Ok(_) => {
            return Err("shutdown: owned product path is not a directory; refusing".to_owned());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "shutdown: owned product dir metadata refused ({e})"
            ));
        }
    }
    // Unknown top-level leaves are residue as well: only the known receipt
    // set plus the residue artifact itself may remain beside them.
    match std::fs::read_dir(run_dir) {
        Ok(entries) => {
            let mut unknown = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name != crate::live_baseline::BASELINE_FILE_NAME
                    && name != RESIDUE_FILE_NAME
                    && name != "product"
                {
                    unknown.push(name);
                }
            }
            if !unknown.is_empty() {
                return Err(format!(
                    "shutdown: owned run dir holds unknown leaves ({}); residue retained, never broad-deleted",
                    unknown.join(",")
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("shutdown: owned run dir already gone ({e})"));
        }
        Err(e) => {
            return Err(format!("shutdown: owned run dir unreadable ({e})"));
        }
    }
    std::fs::remove_dir(run_dir)
        .map_err(|e| format!("shutdown: owned run-dir removal refused ({e})"))
}

/// Remove one known owned leaf (symlink-guarded; absent is fine).
fn remove_known_leaf(dir: &Path, leaf: &str) -> Result<(), String> {
    let path = dir.join(leaf);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!(
            "shutdown: owned run-dir leaf {leaf} is a symlink; refusing"
        )),
        Ok(_) => std::fs::remove_file(&path)
            .map_err(|e| format!("shutdown: owned leaf {leaf} removal refused ({e})")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "shutdown: owned leaf {leaf} metadata refused ({e})"
        )),
    }
}

/// Harness-enforced live leg. Owns the baseline receipt, the starting
/// currents, the run-owned registries (fixture instance from birth,
/// product child handle, owned windows/Spaces/PIDs), and the awake
/// companion lifetime. `finish` runs the full teardown pipeline on every
/// path; `Drop` revokes the admission and runs the same pipeline
/// best-effort with a persisted residue artifact, never claiming proof.
pub struct LiveLeg<'a, T: LegTransport> {
    transport: &'a mut T,
    baseline: LiveBaseline,
    receipt_path: PathBuf,
    generation: u64,
    admission_live: Rc<Cell<bool>>,
    probe_proven: bool,
    finished: bool,
    owned_fixture: Option<FixtureRecord>,
    owned_product: Option<ProductRecord>,
    product_child: Option<std::process::Child>,
    /// Launched-but-unbound runs (fixture and/or product): registered
    /// before `open`, cleared on identity-bound registration. Any entry
    /// still present at `finish` is residue (never falsely clean) and the
    /// receipt is retained for audit.
    pending_launches: Vec<PendingLaunch>,
    owned_windows: Vec<OwnedWindow>,
    owned_spaces: Vec<(usize, u64)>,
    owned_pids: Vec<u32>,
}

impl<'a, T: LegTransport> std::fmt::Debug for LiveLeg<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveLeg")
            .field("generation", &self.generation)
            .field("probe_proven", &self.probe_proven)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl<'a, T: LegTransport> LiveLeg<'a, T> {
    /// Begin a leg: capture every baseline category (any missing category
    /// aborts before anything is touched), persist the receipt atomically
    /// (a persist refusal aborts with zero live effects after it), then hold
    /// the bounded awake companion and prove return-route readiness
    /// (read-only). Fixture/product startup and input wait for registration
    /// plus [`LiveLeg::admit`].
    pub fn begin(transport: &'a mut T, awake_seconds: u64) -> Result<Self, (i32, String)> {
        let baseline = capture_baseline(&mut *transport)
            .map_err(|e| (1, format!("leg-begin baseline: {e}")))?;
        let dir = transport
            .receipt_dir()
            .map_err(|e| (2, format!("leg-begin receipt: {e}")))?;
        let receipt_path = dir.join(crate::live_baseline::BASELINE_FILE_NAME);
        persist_baseline_to(&receipt_path, &baseline)
            .map_err(|e| (2, format!("leg-begin receipt: {e}")))?;
        // From here a receipt exists, so every failure path below unwinds
        // it: nothing ran yet, so there is no residue to audit and the next
        // leg must not be blocked by this one's receipt.
        if let Err(e) = transport.awake_start(awake_seconds) {
            let _ = remove_baseline_receipt(&receipt_path);
            return Err((1, format!("leg-begin awake: {e}")));
        }
        if let Err(e) = transport.return_readiness(&baseline) {
            let _ = transport.awake_stop();
            let _ = remove_baseline_receipt(&receipt_path);
            return Err((1, format!("leg-begin readiness: {e}")));
        }
        Ok(Self {
            transport,
            baseline,
            receipt_path,
            generation: LEG_GENERATION.fetch_add(1, Ordering::SeqCst),
            admission_live: Rc::new(Cell::new(true)),
            probe_proven: false,
            finished: false,
            owned_fixture: None,
            owned_product: None,
            product_child: None,
            pending_launches: Vec::new(),
            owned_windows: Vec::new(),
            owned_spaces: Vec::new(),
            owned_pids: Vec::new(),
        })
    }

    /// The captured baseline (reports and driver joins read through here).
    #[must_use]
    pub fn baseline(&self) -> &LiveBaseline {
        &self.baseline
    }

    /// Mutable transport access for driver legs that drive further
    /// transport reads between admission and `finish`.
    pub fn transport_mut(&mut self) -> &mut T {
        self.transport
    }

    /// Register one launched-but-unbound run BEFORE its `open`: the leg
    /// holds the residue until the native identity joins (via
    /// [`LiveLeg::register_fixture`] / [`LiveLeg::register_product_ls`])
    /// or absence is independently proven (via
    /// [`LiveLeg::resolve_pending_absence`]). On unknown ownership the run
    /// is never killed; the retained run directory is recorded as residue.
    pub fn note_pending_launch(&mut self, pending: PendingLaunch) {
        if !self
            .pending_launches
            .iter()
            .any(|entry| entry.run_dir == pending.run_dir && entry.what == pending.what)
        {
            self.pending_launches.push(pending);
        }
    }

    /// Pending launched-but-unbound runs (fixture and/or product).
    pub fn pending_launches(&self) -> &[PendingLaunch] {
        &self.pending_launches
    }

    /// Attempt to prove one pending run absent without touching foreign
    /// state: the announcement carries no parsable PID or the announced PID
    /// is independently gone, and the control socket file is absent. On
    /// proof the pending entry clears and the owned run directory's known
    /// leaves remove (unknown leaves keep the residue). Anything unprovable
    /// keeps the entry: `finish` reports it as residue and retains the
    /// receipt. Never signals, never kills.
    pub fn resolve_pending_absence(&mut self, what: &'static str) -> Result<(), String> {
        let index = self
            .pending_launches
            .iter()
            .position(|entry| entry.what == what)
            .ok_or_else(|| format!("no pending {what} launch to resolve"))?;
        let pending = self.pending_launches[index].clone();
        let announcement_path = if what == "product" {
            pending.run_dir.join("stdout.log")
        } else {
            pending.run_dir.join("announcement.json")
        };
        let announced_pid = std::fs::read_to_string(&announcement_path)
            .ok()
            .and_then(|raw| {
                raw.lines().find_map(|line| {
                    serde_json::from_str::<serde_json::Value>(line.trim())
                        .ok()
                        .and_then(|value| value.get("pid")?.as_u64())
                        .and_then(|pid| u32::try_from(pid).ok())
                        .filter(|pid| *pid > 1)
                })
            });
        match announced_pid {
            Some(pid) => match self.transport.process_gone(pid) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(format!(
                        "pending {what} launch still present (pid {pid}); residue retained without signals"
                    ));
                }
                Err(e) => {
                    return Err(format!(
                        "pending {what} launch reap unreadable ({e}); residue retained"
                    ));
                }
            },
            None => {
                // No parsable PID: absence is unprovable from identity, so
                // fall through to the socket check only when the run never
                // announced. A missing announcement file plus a missing
                // socket proves nothing launched under our ownership.
            }
        }
        let socket_path = if what == "product" {
            pending.run_dir.join("product.sock")
        } else {
            pending.run_dir.join("fixture.sock")
        };
        if std::fs::symlink_metadata(&socket_path).is_ok() {
            return Err(format!(
                "pending {what} launch socket still present; residue retained without signals"
            ));
        }
        if announced_pid.is_none() && std::fs::symlink_metadata(&announcement_path).is_ok() {
            return Err(format!(
                "pending {what} launch announcement present but unparsable; residue retained"
            ));
        }
        self.pending_launches.remove(index);
        Ok(())
    }

    /// Register the LaunchServices-owned fixture instance from birth (run
    /// nonce verified over its own announcement, executable/launch-date
    /// bound). Exactly one fixture per leg; a second registration refuses.
    /// No admission is possible before this: the controller owns the
    /// instance or fails admission. Clears the matching pending fixture
    /// launch (identity now joined).
    pub fn register_fixture(&mut self, record: FixtureRecord) -> Result<(), (i32, String)> {
        if record.pid <= 1 {
            return Err((
                2,
                format!("leg-register: refusing reserved pid {}", record.pid),
            ));
        }
        if self.owned_fixture.is_some() {
            return Err((
                2,
                "leg-register: fixture already registered; one fixture per leg".to_owned(),
            ));
        }
        self.owned_fixture = Some(record);
        self.pending_launches
            .retain(|entry| entry.what != "fixture");
        Ok(())
    }

    /// Register the leg-spawned product instance plus its exact held Child
    /// handle. The handle (not a PID guess) owns every later signal.
    /// Clears the matching pending product launch.
    pub fn register_product(
        &mut self,
        record: ProductRecord,
        child: std::process::Child,
    ) -> Result<(), (i32, String)> {
        if record.pid <= 1 {
            return Err((
                2,
                format!("leg-register: refusing reserved pid {}", record.pid),
            ));
        }
        if self.owned_product.is_some() {
            return Err((
                2,
                "leg-register: product already registered; one product per leg".to_owned(),
            ));
        }
        self.owned_product = Some(record);
        self.product_child = Some(child);
        self.pending_launches
            .retain(|entry| entry.what != "product");
        Ok(())
    }

    /// Register a LaunchServices-launched product instance (no held
    /// `Child`: the wrapper owns no product lifetime). The bound
    /// executable path plus launch date own every later identity check;
    /// stop rides the stop file plus `outcome.json` evidence plus exact
    /// reaping. Clears the matching pending product launch.
    pub fn register_product_ls(&mut self, record: ProductRecord) -> Result<(), (i32, String)> {
        if record.pid <= 1 {
            return Err((
                2,
                format!("leg-register: refusing reserved pid {}", record.pid),
            ));
        }
        if !record.launch_services {
            return Err((
                2,
                "leg-register: LaunchServices registration needs the launch_services marker"
                    .to_owned(),
            ));
        }
        if record.exe.is_empty() || record.launch.is_empty() {
            return Err((
                2,
                "leg-register: LaunchServices product needs bound executable plus launch date"
                    .to_owned(),
            ));
        }
        if self.owned_product.is_some() {
            return Err((
                2,
                "leg-register: product already registered; one product per leg".to_owned(),
            ));
        }
        self.owned_product = Some(record);
        self.pending_launches
            .retain(|entry| entry.what != "product");
        Ok(())
    }

    /// Register one leg-created native Space for owned-only removal at
    /// `finish` (pinned to its creation display).
    pub fn note_owned_space(&mut self, display: usize, sid: u64) {
        if !self.owned_spaces.contains(&(display, sid)) {
            self.owned_spaces.push((display, sid));
        }
    }

    /// Registered leg-created Spaces (display pins plus SIDs).
    pub fn owned_spaces(&self) -> &[(usize, u64)] {
        &self.owned_spaces
    }

    /// Register one run-owned process for exact-PID reap verification at
    /// `finish`. Never a name, never a guess: the exact PID the leg started.
    pub fn note_owned_pid(&mut self, pid: u32) {
        if !self.owned_pids.contains(&pid) {
            self.owned_pids.push(pid);
        }
    }

    /// Register one run-owned fixture window for per-leg close at `finish`,
    /// keyed by its caller id (the socket close route) plus the joined
    /// native identity for the teardown accounting. Only windows this leg
    /// created are ever registered; pre-existing windows are never touched.
    pub fn note_owned_window(&mut self, caller: &str, pid: u32, wid: u32) {
        if wid != 0
            && !self
                .owned_windows
                .iter()
                .any(|window| window.caller == caller)
        {
            self.owned_windows.push(OwnedWindow {
                caller: caller.to_owned(),
                pid,
                wid,
            });
        }
    }

    /// Execute the pre-mutation return probe on the starting topology: a
    /// newly created owned Space is visited away and back with exact-effect
    /// readbacks, then removed after proving it empty. Must succeed before
    /// any space-mutating command is admitted. A failed probe refuses the
    /// leg (starting return plus owned-temp cleanup attempted, residue
    /// reported); it never falls back to synthetic chords or foreign-target
    /// input.
    pub fn prove_return(&mut self) -> Result<ReturnProof, (i32, String)> {
        let probe = probe_return_to_starting(&mut *self.transport, &self.baseline.native)
            .map_err(|failure| (1, format!("leg return-probe: {failure}")))?;
        let mc_open_after = self.transport.mc_present();
        self.probe_proven = true;
        Ok(ReturnProof {
            probe,
            mc_open_after,
        })
    }

    /// Admit one fixture command path: binds an unforgeable single-leg
    /// admission to the registered fixture PID. Refuses when the controller
    /// owns no such fixture instance, when the chord has no scope, and for
    /// space-mutating chords until the executed return probe succeeded. The
    /// admission is the only key the fixture driver accepts.
    pub fn admit(
        &self,
        fixture_pid: u32,
        chord_name: Option<&str>,
    ) -> Result<LegAdmission, (i32, String)> {
        let owned_pid = self.owned_fixture.as_ref().map(|record| record.pid);
        if owned_pid != Some(fixture_pid) {
            return Err((
                2,
                "leg-admit: controller owns no fixture instance with that PID (register the LaunchServices instance from birth or fail admission)".to_owned(),
            ));
        }
        let scope = match chord_name {
            Some(name) => required_scope(name)
                .ok_or_else(|| (2, format!("leg-admit: unknown --chord '{name}' (see help)")))?,
            None => AdmissionScope::PostOnly,
        };
        if scope == AdmissionScope::SpaceMutating && !self.probe_proven {
            return Err((
                1,
                "leg-admit: space-mutating chords need the executed return probe first (prove_return); refusing"
                    .to_owned(),
            ));
        }
        Ok(LegAdmission::seal(
            fixture_pid,
            scope,
            self.generation,
            Rc::clone(&self.admission_live),
        ))
    }

    /// Stop the registered product: the exact-run stop file first (graceful,
    /// no signals yet), then the ownership-appropriate reaping. Directly
    /// spawned helpers reap through the exact held `Child` handle (never a
    /// PID guess); LaunchServices products (no `Child`: the wrapper owns no
    /// product lifetime) stop through `outcome.json` terminal evidence plus
    /// exact-identity poll reaping. Returns true when reaped. No PID guesses
    /// on any path; every failure collects residue while still attempting
    /// the exact cleanup (no orphaned Child, no signal to unknown ownership).
    fn stop_product(&mut self, residue: &mut Vec<String>) -> bool {
        let Some(record) = self.owned_product.clone() else {
            return true;
        };
        if record.launch_services {
            return self.stop_product_ls(&record, residue);
        }
        let Some(child) = self.product_child.as_mut() else {
            residue.push(format!(
                "owned product pid {} has no held Child handle; residue: product stop unverified",
                record.pid
            ));
            return false;
        };
        // Cooperative stop first: the exact-run stop file is the product's
        // own graceful route (no signals yet). A bounded grace window
        // follows (genuine deadline: the product owns its shutdown latency;
        // stated here).
        let stop_file = record.run_dir.join("stop-requested");
        if let Err(e) = std::fs::write(&stop_file, "stop\n") {
            residue.push(format!(
                "owned product pid {} stop file unwritable ({e}); falling back to the exact handle",
                record.pid
            ));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                Err(e) => {
                    residue.push(format!(
                        "owned product pid {} wait refused ({e}); falling back to the exact handle",
                        record.pid
                    ));
                    break;
                }
            }
        }
        // Exact-handle cleanup on every path: never signals an exited
        // handle, never falls back to a PID guess, reaps only after an
        // observed exit.
        let outcome = crate::session::stop_exact_child(child, record.pid);
        if !outcome.exited {
            residue.push(format!(
                "owned product pid {} did not exit bounded (kill_attempted={}); residue: product not reaped",
                record.pid, outcome.kill_attempted
            ));
        }
        if !outcome.reaped {
            residue.push(format!(
                "owned product pid {} not reaped; residue: child handle unreaped",
                record.pid
            ));
        }
        outcome.exited && outcome.reaped
    }

    /// Stop a LaunchServices-launched product (no held `Child`: the `open`
    /// wrapper owns no product lifetime, so wrapper exit is never native
    /// success): the exact-run stop file first, then `outcome.json`
    /// terminal evidence under a genuine deadline, then exact-identity poll
    /// reaping. An already-exited identity needs no signal; a still-present
    /// one is re-verified (executable plus launch date equal the bound
    /// record) before the exact-PID terminate, and a drifted PID is never
    /// signalled. Returns true only with terminal evidence plus reap.
    fn stop_product_ls(&mut self, record: &ProductRecord, residue: &mut Vec<String>) -> bool {
        let stop_file = record.run_dir.join("stop-requested");
        if let Err(e) = std::fs::write(&stop_file, "stop\n") {
            residue.push(format!(
                "owned product pid {} stop file unwritable ({e}); residue: graceful stop unproven",
                record.pid
            ));
        }
        // Terminal evidence plus liveness under one genuine deadline (the
        // product owns its shutdown latency; stated here): the product
        // publishes its terminal JSON body to `outcome.json` (wrapper exit
        // is never success), then exits. An exit without terminal evidence
        // ends the wait at once (no evidence will appear); a live product
        // without evidence yet keeps polling until the deadline.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut terminal = false;
        let mut gone = false;
        while std::time::Instant::now() < deadline {
            if !terminal
                && let Ok(raw) = std::fs::read_to_string(record.run_dir.join("outcome.json"))
                && serde_json::from_str::<serde_json::Value>(&raw)
                    .map(|value| value.get("outcome").is_some())
                    .unwrap_or(false)
            {
                terminal = true;
            }
            if !gone {
                match self.transport.process_gone(record.pid) {
                    Ok(true) => gone = true,
                    Ok(false) => {}
                    Err(e) => {
                        residue.push(format!(
                            "owned product pid {} reap unreadable ({e}); residue: product stop unverified",
                            record.pid
                        ));
                        return false;
                    }
                }
            }
            if terminal && gone {
                break;
            }
            if gone {
                // Exited without terminal evidence: none will appear.
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if !terminal {
            residue.push(format!(
                "owned product pid {} published no outcome.json terminal evidence past the deadline; residue: completion unproven",
                record.pid
            ));
        }
        if gone {
            return terminal;
        }
        // Still present: re-verify the bound identity before the exact-PID
        // terminate. A drifted PID (relaunch or PID reuse) is never
        // signalled; the residue names it for audit.
        let pid_i32 = i32::try_from(record.pid).unwrap_or(0);
        let live = crate::ffi::ns_app_identity_for_pid(pid_i32);
        match (live.exe, live.launch_date) {
            (Some(exe), Some(launch)) if exe == record.exe && launch == record.launch => {}
            _ => {
                residue.push(format!(
                    "owned product pid {} identity drift (relaunch or PID reuse); refusing signals; residue: foreign process untouched",
                    record.pid
                ));
                return false;
            }
        }
        if !crate::ffi::terminate_process(pid_i32) {
            residue.push(format!(
                "owned product pid {} exact terminate refused; residue: product not reaped",
                record.pid
            ));
            return false;
        }
        let final_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match self.transport.process_gone(record.pid) {
                Ok(true) => return terminal,
                Ok(false) => {}
                Err(e) => {
                    residue.push(format!(
                        "owned product pid {} reap unreadable after terminate ({e}); residue: product stop unverified",
                        record.pid
                    ));
                    return false;
                }
            }
            if std::time::Instant::now() >= final_deadline {
                residue.push(format!(
                    "owned product pid {} still present after exact terminate; residue: product not reaped",
                    record.pid
                ));
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Full teardown pipeline. Every applicable step runs even when earlier
    /// steps fail; failures collect as residue. Order: stop the exact
    /// primary product first (the return below must work after the primary
    /// exits), restore the starting Space, close exactly the registered
    /// fixture windows, shut down and reap only the owned fixture, remove
    /// only leg-created empty Spaces, stop the awake companion, verify
    /// exactly the registered owned PIDs, and only then capture and compare
    /// the teardown (after our own effects are removed).
    fn finish_inner(&mut self) -> TeardownReport {
        let mut residue = Vec::new();
        // 0. Launched-but-unbound runs: any post-open/pre-bind failure
        // (launch spawn, announcement timeout, bundle mismatch, unreadable
        // identity) holds this residue until the native identity joins or
        // absence is independently proven. Never falsely clean, never a
        // signal to unknown ownership.
        for pending in self.pending_launches.clone() {
            residue.push(format!(
                "pending {} launch unverified (expected bundle {}; run attached but identity never joined); residue retained without signals",
                pending.what, pending.expected_bundle
            ));
        }
        // 1. Stop the exact primary product first.
        let product_reaped = self.stop_product(&mut residue);
        // 2. Restore the starting Space (no-op readback when already exact).
        let restore = restore_starting_currents(&mut *self.transport, &self.baseline.native);
        for failure in &restore.failed {
            residue.push(format!("starting-Space restore: {failure}"));
        }
        let current_restored = restore.clean();
        // 3. Close exactly the registered run-owned fixture windows.
        for window in self.owned_windows.clone() {
            if let Err(e) =
                self.transport
                    .close_fixture_window(&window.caller, window.pid, window.wid)
            {
                residue.push(format!(
                    "owned window pid {} wid {} close refused: {e}",
                    window.pid, window.wid
                ));
            }
        }
        // 4. Shut down and reap only the owned fixture (identity-verified,
        // never a signal to a drifted PID).
        if let Some(record) = self.owned_fixture.clone()
            && let Err(e) = self.transport.shutdown_fixture(&record)
        {
            residue.push(format!("owned fixture pid {} shutdown: {e}", record.pid));
        }
        // 5. Remove only leg-created Spaces, each pinned and proven empty.
        for (display, sid) in self.owned_spaces.clone() {
            if let Err(e) = remove_owned_space(&mut *self.transport, display, sid) {
                residue.push(format!("owned Space {sid}: {e}"));
            }
        }
        // 6. Stop the awake companion (exact child, reaped).
        let caffeinate_reaped = match self.transport.awake_stop() {
            Ok(reaped) => reaped,
            Err(e) => {
                residue.push(format!("awake companion stop refused: {e}"));
                false
            }
        };
        // 7. Verify exactly the registered owned PIDs exited. Present PIDs
        // are residue; broad kills by name are never attempted.
        let mut owned_reaped = Vec::new();
        for pid in self.owned_pids.clone() {
            match self.transport.process_gone(pid) {
                Ok(true) => owned_reaped.push(pid),
                Ok(false) => residue.push(format!(
                    "owned pid {pid} still present; residue: process not reaped"
                )),
                Err(e) => residue.push(format!("owned pid {pid} reap unreadable: {e}")),
            }
        }
        // 8. Final capture plus comparison, after our own effects are
        // removed. An unreadable teardown refuses (restoration unprovable);
        // mismatches abort with residue and never overwrite user state.
        let mut mismatches = Vec::new();
        match capture_baseline(&mut *self.transport) {
            Ok(after) => {
                let owned_pairs: Vec<(u32, u32)> = self
                    .owned_windows
                    .iter()
                    .map(|window| (window.pid, window.wid))
                    .collect();
                mismatches = compare_teardown(&self.baseline, &after, &owned_pairs);
            }
            Err(e) => {
                residue.push(format!(
                    "teardown capture unreadable ({e}); restoration unprovable"
                ));
            }
        }
        // 9. Receipt: remove only on a clean full restoration, otherwise
        // retain for audit (retention blocks the next persist until
        // inspected, so residue is never silently dropped). A failure also
        // persists the residue artifact beside the receipt.
        let clean =
            product_reaped && current_restored && mismatches.is_empty() && residue.is_empty();
        let receipt_retained = if clean {
            match remove_baseline_receipt(&self.receipt_path) {
                Ok(()) => false,
                Err(e) => {
                    residue.push(format!("receipt removal refused: {e}"));
                    true
                }
            }
        } else {
            true
        };
        let clean =
            product_reaped && current_restored && mismatches.is_empty() && residue.is_empty();
        if !clean {
            write_residue_artifact(
                &self.receipt_path,
                self.generation,
                &TeardownReport {
                    generation: self.generation,
                    product_reaped,
                    current_restored,
                    restored: restore.restored.clone(),
                    mismatches: mismatches.clone(),
                    residue: residue.clone(),
                    caffeinate_reaped,
                    owned_reaped: owned_reaped.clone(),
                    receipt_retained: true,
                },
            );
        }
        TeardownReport {
            generation: self.generation,
            product_reaped,
            current_restored,
            restored: restore.restored,
            mismatches,
            residue,
            caffeinate_reaped,
            owned_reaped,
            receipt_retained,
        }
    }

    /// Finish the leg: revoke the admission, run the full teardown pipeline,
    /// and report. A non-clean outcome is an error carrying the
    /// metadata-only residue JSON.
    pub fn finish(mut self) -> Result<TeardownReport, (i32, String)> {
        self.admission_live.set(false);
        let report = self.finish_inner();
        self.finished = true;
        let clean = report.product_reaped
            && report.current_restored
            && report.mismatches.is_empty()
            && report.residue.is_empty()
            && !report.receipt_retained;
        if !clean {
            return Err((
                1,
                serde_json::json!({
                    "stage": "leg-finish",
                    "generation": report.generation,
                    "product_reaped": report.product_reaped,
                    "current_restored": report.current_restored,
                    "restored": report.restored,
                    "mismatches": report.mismatches,
                    "residue": report.residue,
                    "caffeinate_reaped": report.caffeinate_reaped,
                    "receipt_retained": report.receipt_retained,
                })
                .to_string(),
            ));
        }
        Ok(report)
    }
}

/// Persist the residue artifact beside the receipt (best-effort; failures
/// are silent here because the receipt itself is always retained on unclean
/// ends, so no evidence is lost).
fn write_residue_artifact(receipt_path: &Path, generation: u64, report: &TeardownReport) {
    let _ = generation;
    if let Some(dir) = receipt_path.parent() {
        let path = dir.join(RESIDUE_FILE_NAME);
        if let Ok(json) = serde_json::to_string(report) {
            let _ = std::fs::write(&path, json);
        }
    }
}

impl<T: LegTransport> Drop for LiveLeg<'_, T> {
    fn drop(&mut self) {
        // Backstop for unwind/interrupt paths that skip `finish`: revoke
        // first, then run the same full pipeline best-effort. Outcomes are
        // unreportable here except through the persisted residue artifact
        // plus the retained receipt; this path never claims proof. Never
        // panics.
        if !self.finished {
            self.admission_live.set(false);
            let report = self.finish_inner();
            let clean = report.product_reaped
                && report.current_restored
                && report.mismatches.is_empty()
                && report.residue.is_empty()
                && !report.receipt_retained;
            if !clean {
                write_residue_artifact(&self.receipt_path, self.generation, &report);
            }
            self.finished = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_baseline::{
        AccentBaseline, HolderBaseline, NativeBaseline, ProcessBaseline, ProtectedWindow,
        load_baseline_from,
    };
    use crate::space_probe::native_baseline_for_test;
    use std::cell::RefCell;
    use std::collections::HashMap;

    fn test_baseline() -> LiveBaseline {
        LiveBaseline {
            version: crate::live_baseline::BASELINE_SCHEMA_VERSION,
            protected_windows: vec![ProtectedWindow {
                pid: 111,
                wid: 7,
                layer: 0,
                onscreen: true,
                x: 10.0,
                y: 20.0,
                w: 800.0,
                h: 600.0,
                bundle: Some("com.example.ghostty-like".to_owned()),
                bundle_protected: false,
                bundle_ghostty: true,
            }],
            holders: (1..=crate::live_baseline::BASELINE_HOLDER_COUNT)
                .map(|desktop| HolderBaseline {
                    symbolic_id: crate::desktop_shortcuts::desktop_symbolic_id(desktop)
                        .expect("holder id"),
                    desktop,
                    enabled: true,
                    keycode: 0x12,
                    modifiers: crate::desktop_shortcuts::CG_EVENT_CTRL_BIT,
                })
                .collect(),
            accent: AccentBaseline {
                present: false,
                rgb: None,
            },
            processes: vec![ProcessBaseline {
                pid: 111,
                bundle: "com.example.ghostty-like".to_owned(),
            }],
            native: native_baseline_for_test(vec![(vec![11, 22], 11)], true),
        }
    }

    /// Scripted leg transport: canned baseline reads, an in-memory Space
    /// table with exact model semantics, and scripted per-step failures for
    /// the fixture/product/awake legs. The probe, restore, and teardown
    /// orchestration under test is the real production code; only the
    /// native table and the side-effect legs are modeled.
    struct ScriptLeg {
        baseline: LiveBaseline,
        fail_category: Option<&'static str>,
        readiness: Result<(), String>,
        spaces: Vec<Vec<u64>>,
        current: Vec<u64>,
        next_sid: u64,
        fail_space_at: Option<&'static str>,
        close_fails: Vec<(u32, u32)>,
        shutdown: Result<(), String>,
        gone: HashMap<u32, Result<bool, String>>,
        awake_stop_reaped: bool,
        receipt_ok: bool,
        order: RefCell<Vec<&'static str>>,
        closes: RefCell<Vec<(String, u32, u32)>>,
        dir: PathBuf,
    }

    impl ScriptLeg {
        fn passing(dir: PathBuf) -> Self {
            let baseline = test_baseline();
            Self {
                baseline,
                fail_category: None,
                readiness: Ok(()),
                spaces: vec![vec![11, 22]],
                current: vec![11],
                next_sid: 1000,
                fail_space_at: None,
                close_fails: Vec::new(),
                shutdown: Ok(()),
                gone: HashMap::new(),
                awake_stop_reaped: true,
                receipt_ok: true,
                order: RefCell::new(Vec::new()),
                closes: RefCell::new(Vec::new()),
                dir,
            }
        }

        fn topo(&self) -> crate::space_probe::SpaceTopology {
            crate::space_probe::SpaceTopology {
                displays: self
                    .spaces
                    .iter()
                    .zip(self.current.iter())
                    .map(
                        |(spaces, current)| crate::space_probe::SpaceTopologyDisplay {
                            spaces: spaces.clone(),
                            current: *current,
                        },
                    )
                    .collect(),
            }
        }
    }

    impl BaselineReads for ScriptLeg {
        fn read_protected_windows(&mut self) -> Result<Vec<ProtectedWindow>, String> {
            self.order.borrow_mut().push("protected");
            if self.fail_category == Some("protected") {
                return Err("injected protected refusal".to_owned());
            }
            Ok(self.baseline.protected_windows.clone())
        }
        fn read_holders(&mut self) -> Result<Vec<HolderBaseline>, String> {
            self.order.borrow_mut().push("holders");
            if self.fail_category == Some("holders") {
                return Err("injected holder refusal".to_owned());
            }
            Ok(self.baseline.holders.clone())
        }
        fn read_accent(&mut self) -> Result<AccentBaseline, String> {
            self.order.borrow_mut().push("accent");
            if self.fail_category == Some("accent") {
                return Err("injected accent refusal".to_owned());
            }
            Ok(self.baseline.accent)
        }
        fn read_processes(&mut self) -> Result<Vec<ProcessBaseline>, String> {
            self.order.borrow_mut().push("processes");
            if self.fail_category == Some("processes") {
                return Err("injected process refusal".to_owned());
            }
            Ok(self.baseline.processes.clone())
        }
        fn read_native(&mut self) -> Result<NativeBaseline, String> {
            self.order.borrow_mut().push("native");
            if self.fail_category == Some("native") {
                return Err("injected native refusal".to_owned());
            }
            Ok(self.baseline.native.clone())
        }
    }

    impl NativeSpaces for ScriptLeg {
        fn space_snapshot(&mut self) -> Result<crate::space_probe::SpaceTopology, String> {
            self.order.borrow_mut().push("snapshot");
            if self.fail_space_at == Some("snapshot") {
                return Err("injected snapshot refusal".to_owned());
            }
            Ok(self.topo())
        }
        fn space_create(&mut self, display: usize) -> Result<u64, String> {
            self.order.borrow_mut().push("create");
            if self.fail_space_at == Some("create") {
                return Err("injected create refusal".to_owned());
            }
            let row = self
                .spaces
                .get_mut(display)
                .ok_or_else(|| format!("model display {display} unknown"))?;
            let temp = self.next_sid;
            self.next_sid += 1;
            row.push(temp);
            Ok(temp)
        }
        fn space_goto(&mut self, display: usize, sid: u64) -> Result<(), String> {
            self.order.borrow_mut().push("goto");
            if self.fail_space_at == Some("goto") {
                return Err("injected goto refusal".to_owned());
            }
            let located = self
                .spaces
                .get(display)
                .is_some_and(|row| row.contains(&sid));
            if !located {
                return Err(format!("model Space {sid} not on display {display}"));
            }
            self.current[display] = sid;
            Ok(())
        }
        fn space_remove(&mut self, sid: u64) -> Result<(), String> {
            self.order.borrow_mut().push("remove");
            if self.fail_space_at == Some("remove") {
                return Err("injected remove refusal".to_owned());
            }
            for row in &mut self.spaces {
                if let Some(index) = row.iter().position(|s| *s == sid) {
                    row.remove(index);
                    return Ok(());
                }
            }
            Err(format!("model Space {sid} unknown"))
        }
        fn space_require_empty(&mut self, _sid: u64) -> Result<(), String> {
            self.order.borrow_mut().push("empty");
            if self.fail_space_at == Some("empty") {
                return Err("model Space occupied".to_owned());
            }
            Ok(())
        }

        fn mc_present(&mut self) -> Option<bool> {
            Some(false)
        }
    }

    impl LegTransport for ScriptLeg {
        fn return_readiness(&mut self, _starting: &LiveBaseline) -> Result<(), String> {
            self.order.borrow_mut().push("readiness");
            self.readiness.clone()
        }
        fn close_fixture_window(&mut self, caller: &str, pid: u32, wid: u32) -> Result<(), String> {
            self.order.borrow_mut().push("close");
            self.closes.borrow_mut().push((caller.to_owned(), pid, wid));
            if self.close_fails.contains(&(pid, wid)) {
                return Err("injected close refusal".to_owned());
            }
            Ok(())
        }
        fn shutdown_fixture(&mut self, record: &FixtureRecord) -> Result<(), String> {
            self.order.borrow_mut().push("shutdown");
            // Identity-bounded like production: a drifted record refuses
            // before any control-channel touch.
            if record.exe != "/live-exe" || record.launch != "launch-live" {
                return Err(format!(
                    "shutdown: fixture pid {} identity drift; refusing",
                    record.pid
                ));
            }
            self.shutdown.clone()
        }
        fn process_gone(&mut self, pid: u32) -> Result<bool, String> {
            self.order.borrow_mut().push("reap");
            match self.gone.get(&pid) {
                None => Ok(true),
                Some(Ok(gone)) => Ok(*gone),
                Some(Err(e)) => Err(e.clone()),
            }
        }
        fn awake_start(&mut self, _seconds: u64) -> Result<(), String> {
            self.order.borrow_mut().push("awake-start");
            Ok(())
        }
        fn awake_stop(&mut self) -> Result<bool, String> {
            self.order.borrow_mut().push("awake-stop");
            Ok(self.awake_stop_reaped)
        }
        fn receipt_dir(&mut self) -> Result<PathBuf, String> {
            if self.receipt_ok {
                Ok(self.dir.clone())
            } else {
                Err("no home; refusing".to_owned())
            }
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("leg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test dir creates");
        dir
    }

    fn test_fixture() -> FixtureRecord {
        FixtureRecord {
            pid: 4242,
            exe: "/live-exe".to_owned(),
            launch: "launch-live".to_owned(),
            socket_path: "/tmp/owned-fixture/fixture.sock".to_owned(),
            run_dir: PathBuf::from("/tmp/owned-fixture"),
        }
    }

    #[test]
    fn chord_scopes_split_membership_from_selection() {
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
            assert_eq!(
                required_scope(name),
                Some(AdmissionScope::PostOnly),
                "{name}"
            );
        }
        for name in [
            "select-0",
            "select-1",
            "select-9",
            "send-0",
            "send-1",
            "send-9",
            "rel-prev",
            "rel-next",
            "prev-toggle",
        ] {
            assert_eq!(
                required_scope(name),
                Some(AdmissionScope::SpaceMutating),
                "{name}"
            );
        }
        for name in ["", "stay-1", "escape", "select-10", "workspace-select-1"] {
            assert_eq!(required_scope(name), None, "{name}");
        }
        assert!(AdmissionScope::SpaceMutating.covers(AdmissionScope::PostOnly));
        assert!(AdmissionScope::SpaceMutating.covers(AdmissionScope::SpaceMutating));
        assert!(!AdmissionScope::PostOnly.covers(AdmissionScope::SpaceMutating));
        assert!(AdmissionScope::PostOnly.covers(AdmissionScope::PostOnly));
    }

    #[test]
    fn begin_orders_capture_persist_awake_readiness() {
        let dir = test_dir("begin");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        assert_eq!(leg.baseline(), &test_baseline());
        assert!(!leg.probe_proven);
        assert_eq!(
            *leg.transport_mut().order.borrow(),
            vec![
                "protected",
                "holders",
                "accent",
                "processes",
                "native",
                "awake-start",
                "readiness",
            ]
        );
        let receipt = dir.join(crate::live_baseline::BASELINE_FILE_NAME);
        assert_eq!(
            load_baseline_from(&receipt).expect("receipt persists"),
            test_baseline()
        );
        let report = leg.finish().expect("clean finish passes");
        assert!(report.current_restored);
        assert!(report.mismatches.is_empty());
        assert!(report.residue.is_empty());
        assert!(report.caffeinate_reaped);
        assert!(!receipt.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_missing_baseline_category_blocks_before_effects() {
        for category in ["protected", "holders", "accent", "processes", "native"] {
            let dir = test_dir(category);
            let mut transport = ScriptLeg::passing(dir.clone());
            transport.fail_category = Some(category);
            let err = LiveLeg::begin(&mut transport, 60).unwrap_err();
            assert_eq!(err.0, 1, "{category}");
            assert!(err.1.contains("baseline"), "{category}: {}", err.1);
            assert!(
                !transport.order.borrow().contains(&"awake-start"),
                "{category}"
            );
            assert!(
                !transport.order.borrow().contains(&"readiness"),
                "{category}"
            );
            assert!(!dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn receipt_and_readiness_failures_abort_with_zero_input() {
        let dir = test_dir("nohome");
        let mut transport = ScriptLeg::passing(dir);
        transport.receipt_ok = false;
        let err = LiveLeg::begin(&mut transport, 60).unwrap_err();
        assert_eq!(err.0, 2, "unexpected: {}", err.1);
        assert!(!transport.order.borrow().contains(&"awake-start"));
        let dir = test_dir("readiness");
        let mut transport = ScriptLeg::passing(dir.clone());
        transport.readiness = Err("Dock identity ambiguous (2 entries)".to_owned());
        let err = LiveLeg::begin(&mut transport, 60).unwrap_err();
        assert_eq!(err.0, 1, "unexpected: {}", err.1);
        assert!(err.1.contains("readiness"), "unexpected: {}", err.1);
        assert_eq!(
            *transport.order.borrow(),
            vec![
                "protected",
                "holders",
                "accent",
                "processes",
                "native",
                "awake-start",
                "readiness",
                "awake-stop",
            ]
        );
        assert!(!dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn admit_requires_the_registered_fixture_instance() {
        // No registration: even the exact announced PID cannot admit (the
        // controller owns no instance from birth).
        let dir = test_dir("admit-reg");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        let err = leg.admit(4242, Some("focus-h")).unwrap_err();
        assert_eq!(err.0, 2, "unexpected: {}", err.1);
        assert!(err.1.contains("owns no fixture"), "unexpected: {}", err.1);
        // A foreign PID never admits either.
        leg.register_fixture(test_fixture()).expect("registers");
        let err = leg.admit(7777, Some("focus-h")).unwrap_err();
        assert_eq!(err.0, 2, "unexpected: {}", err.1);
        leg.finish().expect("clean finish passes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn space_mutating_chords_need_the_executed_probe() {
        let dir = test_dir("admit");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.register_fixture(test_fixture()).expect("registers");
        let post = leg.admit(4242, Some("focus-h")).expect("post-only admits");
        assert_eq!(post.fixture_pid(), 4242);
        assert_eq!(post.scope(), AdmissionScope::PostOnly);
        let err = leg.admit(4242, Some("select-1")).unwrap_err();
        assert_eq!(err.0, 1, "unexpected: {}", err.1);
        assert!(err.1.contains("return probe"), "unexpected: {}", err.1);
        let proof = leg.prove_return().expect("probe proves");
        assert_eq!(
            proof.probe.steps,
            vec![
                "snapshot-before",
                "create-temp",
                "verify-append",
                "temp-empty",
                "goto-away",
                "goto-back",
                "remove-temp",
                "verify-currents",
            ]
        );
        let space = leg.admit(4242, Some("select-1")).expect("space admits");
        assert_eq!(space.scope(), AdmissionScope::SpaceMutating);
        assert_eq!(
            leg.admit(4242, Some("rel-prev"))
                .expect("bound admits")
                .scope(),
            AdmissionScope::SpaceMutating
        );
        assert_eq!(leg.admit(4242, Some("stay-1")).unwrap_err().0, 2);
        assert_eq!(
            leg.admit(4242, None).expect("focus admits").scope(),
            AdmissionScope::PostOnly
        );
        leg.finish().expect("clean finish passes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_probe_attempts_return_and_cleanup() {
        // A failed away-goto still attempts the starting return and the
        // owned-temp removal before refusing with residue.
        let dir = test_dir("probefail");
        let mut transport = ScriptLeg::passing(dir.clone());
        transport.fail_space_at = Some("goto");
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.register_fixture(test_fixture()).expect("registers");
        // Fail only the away goto: the entry restore and cleanup gotos
        // succeed, so the temp is still removed.
        let err = leg.prove_return().unwrap_err();
        assert_eq!(err.0, 1, "unexpected: {}", err.1);
        assert!(!leg.probe_proven);
        let err = leg.admit(4242, Some("send-2")).unwrap_err();
        assert!(err.1.contains("return probe"), "unexpected: {}", err.1);
        assert!(leg.admit(4242, Some("focus-h")).is_ok());
        // Drift away, then finish: the restore re-attempt fails (scripted)
        // and the leg reports the residue with the receipt retained.
        leg.transport_mut().current[0] = 22;
        let outcome = leg.finish();
        assert!(outcome.is_err(), "failed restore must not pass");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn admissions_revoke_on_finish_and_never_replay() {
        let dir = test_dir("revoke");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.register_fixture(test_fixture()).expect("registers");
        let admission = leg.admit(4242, Some("focus-h")).expect("admits");
        assert!(admission.is_live());
        let generation = admission.generation();
        assert!(generation >= 1);
        leg.finish().expect("clean finish passes");
        // Use-after-finish refuses: the token is revoked.
        assert!(!admission.is_live());
        // A fresh leg mints a fresh generation: the old admission binds
        // nothing new (its token is dead).
        let mut transport2 = ScriptLeg::passing(dir.clone());
        let mut leg2 = LiveLeg::begin(&mut transport2, 60).expect("leg begins");
        leg2.register_fixture(test_fixture()).expect("registers");
        let admission2 = leg2.admit(4242, Some("focus-h")).expect("admits");
        assert!(admission2.is_live());
        assert_ne!(admission2.generation(), admission.generation());
        leg2.finish().expect("clean finish passes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finish_runs_every_step_despite_each_failure() {
        // Every teardown leg fails: the pipeline still attempts all
        // remaining safe steps, collects the full residue, and retains the
        // receipt. Order pins product-stop first, awake-stop before the
        // final capture/compare.
        let dir = test_dir("finish-all-fail");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.register_fixture(test_fixture()).expect("registers");
        leg.prove_return().expect("probe proves");
        leg.note_owned_window("a", 4242, 81);
        leg.note_owned_pid(4242);
        leg.note_owned_space(0, 555);
        assert_eq!(leg.owned_spaces(), &[(0, 555)]);
        leg.transport_mut().current[0] = 22;
        leg.transport_mut().fail_space_at = Some("goto");
        leg.transport_mut().close_fails.push((4242, 81));
        leg.transport_mut().shutdown = Err("socket wedged".to_owned());
        leg.transport_mut().gone.insert(4242, Ok(false));
        leg.transport_mut().awake_stop_reaped = false;
        leg.transport_mut().baseline.protected_windows[0].w = 801.0;
        // Block the borrow: finish consumes the leg, releasing transport.
        let err = leg.finish().unwrap_err();
        assert_eq!(err.0, 1, "unexpected: {}", err.1);
        assert!(err.1.contains("leg-finish"), "unexpected: {}", err.1);
        for marker in [
            "restore",
            "close refused",
            "shutdown",
            "ownership uncertain",
            "still present",
            "protected-window",
        ] {
            assert!(err.1.contains(marker), "residue lacks {marker}: {}", err.1);
        }
        assert!(dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
        assert!(dir.join(RESIDUE_FILE_NAME).exists());
        // Order pins product-stop first and awake-stop before the final
        // capture/compare: restore goto precedes the owned close, which
        // precedes shutdown, which precedes the last awake stop.
        let order = transport.order.borrow().clone();
        let goto_at = order.iter().position(|leg| *leg == "goto").expect("goto");
        let close_at = order.iter().position(|leg| *leg == "close").expect("close");
        let shutdown_at = order
            .iter()
            .position(|leg| *leg == "shutdown")
            .expect("shutdown");
        let stop_at = order
            .iter()
            .rposition(|leg| *leg == "awake-stop")
            .expect("stop");
        assert!(
            goto_at < close_at && close_at < shutdown_at && shutdown_at < stop_at,
            "finish order is restore -> close owned -> shutdown -> stop awake: {order:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_runs_the_pipeline_and_persists_residue() {
        // Unwind path without finish: Drop revokes, restores, compares,
        // retains the receipt, and persists the residue artifact, never
        // claiming proof.
        let dir = test_dir("drop");
        let receipt = dir.join(crate::live_baseline::BASELINE_FILE_NAME);
        let residue_path = dir.join(RESIDUE_FILE_NAME);
        let admission = {
            let mut transport = ScriptLeg::passing(dir.clone());
            let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
            leg.register_fixture(test_fixture()).expect("registers");
            let admission = leg.admit(4242, Some("focus-h")).expect("admits");
            leg.transport_mut().baseline.protected_windows[0].w = 801.0;
            admission
        };
        // Dropped without finish: the admission is revoked and the receipt
        // plus the residue artifact remain for audit.
        assert!(!admission.is_live());
        assert!(receipt.exists());
        assert!(residue_path.exists());
        let residue: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&residue_path).expect("residue reads"))
                .expect("residue parses");
        assert!(residue.get("mismatches").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn teardown_report_carries_ids_only() {
        let report = TeardownReport {
            generation: 7,
            product_reaped: true,
            current_restored: false,
            restored: vec![(0, 11)],
            mismatches: vec![BaselineMismatch {
                category: "native-topology".to_owned(),
                detail: "display 0 current Space 22 != starting 11".to_owned(),
            }],
            residue: vec!["owned pid 4242 still present".to_owned()],
            caffeinate_reaped: true,
            owned_reaped: vec![],
            receipt_retained: true,
        };
        let flat = serde_json::to_string(&report).expect("report flattens");
        for leaked in [
            "title",
            "nonce",
            "path",
            "executable",
            "requirement",
            "signer",
            "socket",
        ] {
            assert!(!flat.contains(leaked), "schema leaks {leaked}");
        }
    }

    fn pending_fixture(dir: &std::path::Path) -> PendingLaunch {
        PendingLaunch {
            run_dir: dir.to_path_buf(),
            expected_bundle: "io.github.omnitiler.live-fixture".to_owned(),
            what: "fixture",
        }
    }

    #[test]
    fn every_post_open_pre_bind_failure_cannot_clean() {
        // A launched-but-unbound run (open succeeded, then announcement /
        // identity / bundle checks fail before registration) holds residue
        // at finish: the leg reports unclean with the receipt retained and
        // never claims a clean teardown. No kill is attempted on unknown
        // ownership (the transport order shows no shutdown signal path).
        let dir = test_dir("pending");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.note_pending_launch(pending_fixture(&dir));
        let err = leg.finish().unwrap_err();
        assert_eq!(err.0, 1, "unexpected: {}", err.1);
        assert!(
            err.1.contains("pending fixture launch unverified"),
            "unexpected: {}",
            err.1
        );
        assert!(
            err.1.contains("without signals"),
            "no kill on unknown ownership: {}",
            err.1
        );
        assert!(dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
        assert!(dir.join(RESIDUE_FILE_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proven_pending_absence_clears_before_finish() {
        // No announcement, no socket: absence is independently proven, so
        // the pending entry clears and the leg finishes clean.
        let dir = test_dir("pending-absent");
        let run = dir.join("run-absent");
        std::fs::create_dir_all(&run).expect("run dir creates");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.note_pending_launch(pending_fixture(&run));
        leg.resolve_pending_absence("fixture")
            .expect("absence proves without identity");
        assert!(leg.pending_launches().is_empty());
        leg.finish().expect("clean finish passes");
        assert!(!dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_present_process_blocks_absence_and_finish() {
        // An announced PID that is still present blocks the absence proof
        // (no signals sent) and blocks a clean finish.
        let dir = test_dir("pending-present");
        let run = dir.join("run-present");
        std::fs::create_dir_all(&run).expect("run dir creates");
        let pid = 4242u32;
        std::fs::write(
            run.join("announcement.json"),
            format!(
                "{{\"socket\":\"{}/fixture.sock\",\"pid\":{pid},\"nonce\":\"{}\"}}",
                run.display(),
                "a".repeat(64)
            ),
        )
        .expect("announcement writes");
        let mut transport = ScriptLeg::passing(dir.clone());
        transport.gone.insert(pid, Ok(false));
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.note_pending_launch(pending_fixture(&run));
        let err = leg.resolve_pending_absence("fixture").unwrap_err();
        assert!(err.contains("still present"), "unexpected: {err}");
        assert!(err.contains("without signals"), "unexpected: {err}");
        let err = leg.finish().unwrap_err();
        assert!(
            err.1.contains("pending fixture launch unverified"),
            "unexpected: {}",
            err.1
        );
        assert!(dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn owned_run_dir_removal_takes_only_known_leaves() {
        // A real filesystem run dir populated with the known fixture plus
        // product outcome leaves removes fully.
        let dir = test_dir("rundir-known");
        let run = dir.join("run");
        let product = run.join("product");
        std::fs::create_dir_all(&product).expect("product dir creates");
        for leaf in ["announcement.json", "stderr.log", "fixture.sock"] {
            std::fs::write(run.join(leaf), "known\n").expect("leaf writes");
        }
        for leaf in [
            "stop-requested",
            "outcome.json",
            "stdout.log",
            "stderr.log",
            "run-id",
        ] {
            std::fs::write(product.join(leaf), "known\n").expect("leaf writes");
        }
        remove_owned_run_dir(&run).expect("known leaves remove");
        assert!(!run.exists());
        // An unknown leaf is residue, never broad-deleted: the removal
        // refuses and the unknown file survives for audit.
        let run = dir.join("run-unknown");
        let product = run.join("product");
        std::fs::create_dir_all(&product).expect("product dir creates");
        std::fs::write(run.join("announcement.json"), "known\n").expect("leaf writes");
        std::fs::write(product.join("outcome.json"), "known\n").expect("leaf writes");
        std::fs::write(product.join("foreign-debug.log"), "unknown\n").expect("leaf writes");
        let err = remove_owned_run_dir(&run).unwrap_err();
        assert!(err.contains("unknown leaves"), "unexpected: {err}");
        assert!(run.join("product").join("foreign-debug.log").exists());
        // A symlinked leaf refuses rather than following.
        let run = dir.join("run-link");
        std::fs::create_dir_all(&run).expect("run dir creates");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/tmp", run.join("fixture.sock")).expect("link creates");
        #[cfg(unix)]
        {
            let err = remove_owned_run_dir(&run).unwrap_err();
            assert!(err.contains("symlink"), "unexpected: {err}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ls_product_stop_uses_terminal_evidence_and_unblocks_repeat_begin() {
        // A LaunchServices product with `outcome.json` terminal evidence
        // and an already-gone PID stops clean: the stop file is written,
        // the receipt removes, and a repeat begin is unblocked.
        let dir = test_dir("ls-product");
        let product = dir.join("product");
        std::fs::create_dir_all(&product).expect("product dir creates");
        std::fs::write(
            product.join("outcome.json"),
            "{\"outcome\":\"completed\",\"shutdown_clean\":true}",
        )
        .expect("outcome writes");
        let mut transport = ScriptLeg::passing(dir.clone());
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.register_product_ls(ProductRecord {
            pid: 4243,
            exe: "/probe-exe".to_owned(),
            launch: "launch-product".to_owned(),
            run_dir: product.clone(),
            launch_services: true,
        })
        .expect("LS product registers");
        assert!(leg.pending_launches().is_empty());
        leg.finish().expect("clean finish passes");
        assert!(product.join("stop-requested").exists());
        assert!(!dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
        // Repeat begin is unblocked after the successful receipt removal.
        let mut transport = ScriptLeg::passing(dir.clone());
        LiveLeg::begin(&mut transport, 60).expect("repeat begin passes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ls_product_without_terminal_evidence_retains_receipt() {
        // No `outcome.json` with an already-gone PID: the stop reports
        // residue fast (the exit proves no evidence will appear) and
        // retains the receipt.
        let dir = test_dir("ls-product-dirty");
        let product = dir.join("product");
        std::fs::create_dir_all(&product).expect("product dir creates");
        let mut transport = ScriptLeg::passing(dir.clone());
        transport.gone.insert(4244, Ok(true));
        let mut leg = LiveLeg::begin(&mut transport, 60).expect("leg begins");
        leg.register_product_ls(ProductRecord {
            pid: 4244,
            exe: "/probe-exe".to_owned(),
            launch: "launch-product".to_owned(),
            run_dir: product.clone(),
            launch_services: true,
        })
        .expect("LS product registers");
        let err = leg.finish().unwrap_err();
        assert!(err.1.contains("outcome.json"), "unexpected: {}", err.1);
        assert!(dir.join(crate::live_baseline::BASELINE_FILE_NAME).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
