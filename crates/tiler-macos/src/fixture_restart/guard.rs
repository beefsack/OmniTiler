//! The ordered between-launch guard plus the read-only checkpoint it judges:
//! launch identity, overlap, the first launch's terminal/PID/lifetime
//! disappearance, the authenticated fixture and join, the owned Space and
//! window placement, and the exact store fingerprint. The first failing
//! stage refuses; reads are native/socket probes only.

use std::path::Path;

use crate::fixture_leg::{FIXTURE_SCOPE, join_fixture_windows, read_run_nonce};
use crate::fixture_owned_space::{PostOnlyOwnedSpace, await_joined_on_owned_space};
use crate::live_leg::{
    FixtureRecord, LegTransport, LiveLeg, LiveLegTransport, RestartStopEvidence,
    clean_terminal_verdict,
};
use crate::space_probe::NativeSpaces;

use super::store::{StoreSnapshot, read_store_snapshot};

/// Liveness read for one exact PID: gone, present, or unreadable (never a
/// guess). Pure data; the guard judges it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Liveness {
    Gone,
    Present,
    Unreadable(String),
}

/// Guard stage library (ordered): the first failing fact refuses the second
/// launch, so a later clean fact can never mask an earlier uncertainty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardStage {
    LaunchIdentity,
    ProductOverlap,
    PriorTerminal,
    PriorPid,
    PriorLifetime,
    FixturePid,
    FixtureIdentity,
    FixtureNonce,
    FixtureJoin,
    OwnedSpace,
    WindowPlacement,
    Store,
}

impl GuardStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LaunchIdentity => "launch-identity",
            Self::ProductOverlap => "product-overlap",
            Self::PriorTerminal => "prior-terminal",
            Self::PriorPid => "prior-pid",
            Self::PriorLifetime => "prior-lifetime",
            Self::FixturePid => "fixture-pid",
            Self::FixtureIdentity => "fixture-identity",
            Self::FixtureNonce => "fixture-nonce",
            Self::FixtureJoin => "fixture-join",
            Self::OwnedSpace => "owned-space",
            Self::WindowPlacement => "window-placement",
            Self::Store => "store",
        }
    }
}

/// Plain-data observations for the immediately-before-second-launch guard.
/// Every field is a read fact; the verdict is pure.
#[derive(Debug, Clone)]
pub(crate) struct RestartGuardInputs {
    pub launch_ids_distinct: bool,
    pub product_overlap: bool,
    pub product_overlap_note: Option<String>,
    pub prior_terminal_clean: bool,
    pub prior_liveness: Liveness,
    pub prior_lifetime_gone: bool,
    pub fixture_liveness: Liveness,
    pub fixture_identity_readable: bool,
    pub fixture_bundle_match: bool,
    pub fixture_exe_match: bool,
    pub fixture_launch_match: bool,
    pub fixture_nonce_match: bool,
    pub fixture_join_same: bool,
    pub fixture_join_note: Option<String>,
    pub topology_readable: bool,
    pub current_owned_space: bool,
    pub topology_row_exact: bool,
    pub windows_on_owned_space: bool,
    pub store_expected: StoreSnapshot,
    pub store_observed: StoreSnapshot,
}

impl RestartGuardInputs {
    /// Metadata-only evidence (no paths, launch tokens, nonces, titles, or
    /// requirement text).
    pub(crate) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "launch_ids_distinct": self.launch_ids_distinct,
            "product_overlap": self.product_overlap,
            "product_overlap_note": self.product_overlap_note,
            "prior_terminal_clean": self.prior_terminal_clean,
            "prior_liveness": liveness_evidence(&self.prior_liveness),
            "prior_lifetime_gone": self.prior_lifetime_gone,
            "fixture_liveness": liveness_evidence(&self.fixture_liveness),
            "fixture_identity_readable": self.fixture_identity_readable,
            "fixture_bundle_match": self.fixture_bundle_match,
            "fixture_exe_match": self.fixture_exe_match,
            "fixture_launch_match": self.fixture_launch_match,
            "fixture_nonce_match": self.fixture_nonce_match,
            "fixture_join_same": self.fixture_join_same,
            "fixture_join_note": self.fixture_join_note,
            "topology_readable": self.topology_readable,
            "current_owned_space": self.current_owned_space,
            "topology_row_exact": self.topology_row_exact,
            "windows_on_owned_space": self.windows_on_owned_space,
            "store_expected": self.store_expected.evidence(),
            "store_observed": self.store_observed.evidence(),
        })
    }
}

fn liveness_evidence(liveness: &Liveness) -> serde_json::Value {
    match liveness {
        Liveness::Gone => serde_json::json!({"state": "gone"}),
        Liveness::Present => serde_json::json!({"state": "present"}),
        Liveness::Unreadable(error) => {
            serde_json::json!({"state": "unreadable", "error": error})
        }
    }
}

fn guard_refusal(stage: GuardStage, detail: &str) -> String {
    format!("restart-guard {}: {detail}", stage.as_str())
}

/// Pure ordered restart guard. Checks the static launch identity, overlap,
/// the first launch's clean terminal evidence plus independent PID/lifetime
/// disappearance, the authenticated fixture lifetime and join, the owned
/// Space/topology/window placement, and finally exact store bytes plus
/// metadata. The first failing stage refuses; no later fact can mask it.
/// Pure; unit-tested.
pub(crate) fn restart_guard_verdict(inputs: &RestartGuardInputs) -> Result<(), String> {
    if !inputs.launch_ids_distinct {
        return Err(guard_refusal(
            GuardStage::LaunchIdentity,
            "the two launches share one run ID or output directory; refusing",
        ));
    }
    if inputs.product_overlap {
        return Err(guard_refusal(
            GuardStage::ProductOverlap,
            "another owned or pending product instance exists; refusing the second launch",
        ));
    }
    if !inputs.prior_terminal_clean {
        return Err(guard_refusal(
            GuardStage::PriorTerminal,
            "the first launch has no clean terminal evidence; refusing the second launch",
        ));
    }
    match &inputs.prior_liveness {
        Liveness::Gone => {}
        Liveness::Present => {
            return Err(guard_refusal(
                GuardStage::PriorPid,
                "the first product PID is still present; refusing overlapping launches",
            ));
        }
        Liveness::Unreadable(error) => {
            return Err(guard_refusal(
                GuardStage::PriorPid,
                &format!("the first product liveness is unreadable ({error}); refusing"),
            ));
        }
    }
    if !inputs.prior_lifetime_gone {
        return Err(guard_refusal(
            GuardStage::PriorLifetime,
            "the first product's bound executable/launch identity is still live; refusing",
        ));
    }
    match &inputs.fixture_liveness {
        Liveness::Present => {}
        Liveness::Gone => {
            return Err(guard_refusal(
                GuardStage::FixturePid,
                "the authenticated fixture PID is gone; refusing the second launch",
            ));
        }
        Liveness::Unreadable(error) => {
            return Err(guard_refusal(
                GuardStage::FixturePid,
                &format!("fixture liveness is unreadable ({error}); refusing"),
            ));
        }
    }
    if !inputs.fixture_identity_readable {
        return Err(guard_refusal(
            GuardStage::FixtureIdentity,
            "fixture identity is unreadable; refusing",
        ));
    }
    if !(inputs.fixture_bundle_match && inputs.fixture_exe_match && inputs.fixture_launch_match) {
        return Err(guard_refusal(
            GuardStage::FixtureIdentity,
            "fixture bundle/executable/launch identity drifted; refusing",
        ));
    }
    if !inputs.fixture_nonce_match {
        return Err(guard_refusal(
            GuardStage::FixtureNonce,
            "fixture announcement nonce changed; refusing",
        ));
    }
    if !inputs.fixture_join_same {
        return Err(guard_refusal(
            GuardStage::FixtureJoin,
            &format!(
                "authenticated fixture window join drifted{}; refusing",
                inputs
                    .fixture_join_note
                    .as_deref()
                    .map(|note| format!(" ({note})"))
                    .unwrap_or_default()
            ),
        ));
    }
    if !inputs.topology_readable {
        return Err(guard_refusal(
            GuardStage::OwnedSpace,
            "owned-Space topology is unreadable; refusing",
        ));
    }
    if !(inputs.current_owned_space && inputs.topology_row_exact) {
        return Err(guard_refusal(
            GuardStage::OwnedSpace,
            "the owned Space is no longer current or the row changed; refusing",
        ));
    }
    if !inputs.windows_on_owned_space {
        return Err(guard_refusal(
            GuardStage::WindowPlacement,
            "an owned window is no longer readable on the owned Space; refusing",
        ));
    }
    if inputs.store_expected != inputs.store_observed {
        return Err(guard_refusal(
            GuardStage::Store,
            "the isolated store bytes or metadata changed before the second launch; refusing",
        ));
    }
    Ok(())
}

/// Exact owned-Space facts shared by the pre-launch guard and the
/// post-restart capture: topology readable, owned SID current, and the
/// display row exactly the owned row. One reader, two enforcement points.
#[cfg(target_os = "macos")]
pub(crate) fn owned_topology_facts(
    transport: &mut LiveLegTransport,
    owned: &PostOnlyOwnedSpace,
) -> (bool, bool, bool) {
    match transport.space_snapshot() {
        Ok(topology) => match topology.displays.first() {
            Some(row) => (
                true,
                row.current == owned.sid(),
                row.spaces.as_slice() == owned.spaces_after(),
            ),
            None => (false, false, false),
        },
        Err(_) => (false, false, false),
    }
}

/// Pure app-table test for an active foreign product instance. The
/// controller itself is excluded by exact PID, and only live processes
/// count; a terminated table entry is not an active instance.
#[cfg(target_os = "macos")]
pub(crate) fn foreign_product_in_apps(
    apps: &[(i32, String)],
    self_pid: i32,
    product_bundle: &str,
    alive: impl Fn(i32) -> bool,
) -> bool {
    apps.iter()
        .any(|(pid, bundle)| *pid != self_pid && bundle.as_str() == product_bundle && alive(*pid))
}

/// Fail-closed overlap proof over the live app table plus CG window owners:
/// any active product bundle instance other than the controller refuses the
/// restart, including a foreign product tile whose owner is still alive.
/// An unreadable/empty app table or CG list is an error, never "no overlap".
#[cfg(target_os = "macos")]
pub(crate) fn foreign_product_overlap(
    leg: &LiveLeg<LiveLegTransport>,
    fixture_pid: u32,
) -> Result<bool, String> {
    if leg.has_owned_product()
        || leg
            .pending_launches()
            .iter()
            .any(|pending| pending.what == "product")
    {
        return Ok(true);
    }
    let apps = crate::ffi::running_apps();
    if apps.is_empty() {
        return Err(
            "running-application table unreadable or empty; refusing overlap proof".to_owned(),
        );
    }
    let self_pid = i32::try_from(std::process::id()).unwrap_or(-1);
    if foreign_product_in_apps(&apps, self_pid, crate::BUNDLE_ID, |pid| {
        crate::ffi::process_exists(pid)
    }) {
        return Ok(true);
    }
    let cg = crate::ffi::cg_window_list_basic()
        .map_err(|error| format!("CG window list unreadable ({error}); refusing overlap proof"))?;
    for window in cg {
        if window.pid == std::process::id() || window.pid == fixture_pid {
            continue;
        }
        let pid = i32::try_from(window.pid).unwrap_or(-1);
        if pid <= 1 || !crate::ffi::process_exists(pid) {
            continue;
        }
        if crate::ffi::bundle_id_for_pid(pid).as_deref() == Some(crate::BUNDLE_ID) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Re-verify every identity and placement fact at the post-restart capture:
/// the registered second product is the exact live PID/lifetime, the
/// fixture bundle/executable/launch and nonce still bind, the joined window
/// set is unchanged, the owned Space is current with an exact topology row,
/// and every joined window is readable on it. Any uncertainty refuses.
#[cfg(target_os = "macos")]
pub(crate) fn require_post_restart_capture_bound(
    leg: &mut LiveLeg<LiveLegTransport>,
    context: OwnedRestartFixture<'_>,
    launch2_pid: u32,
    launch2_exe: &str,
    launch2_launch: &str,
) -> Result<serde_json::Value, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    let owned = context.owned;
    let record_exact = leg.owned_product_record().is_some_and(|record| {
        record.pid == launch2_pid
            && record.exe == launch2_exe
            && record.launch == launch2_launch
            && record.launch_services
    });
    let product_identity =
        crate::ffi::ns_app_identity_for_pid(i32::try_from(launch2_pid).unwrap_or(0));
    let product_live_exact = product_identity.exe.as_deref() == Some(launch2_exe)
        && product_identity.launch_date.as_deref() == Some(launch2_launch);
    let fixture_identity = crate::fixture_post::fixture_process_identity(fixture.pid);
    let (fixture_identity_readable, fixture_bundle_match, fixture_exe_match, fixture_launch_match) =
        match fixture_identity {
            Ok(identity) => (
                true,
                identity.bundle.as_deref() == Some(FIXTURE_SCOPE),
                identity.exe.as_deref() == Some(fixture.exe.as_str()),
                identity.launch.as_deref() == Some(fixture.launch.as_str()),
            ),
            Err(_) => (false, false, false, false),
        };
    let fixture_nonce_match =
        read_run_nonce(&fixture.run_dir).is_ok_and(|live| live == context.nonce);
    let callers: Vec<String> = joined.iter().map(|(caller, _)| caller.clone()).collect();
    let fixture_join_same =
        join_fixture_windows(
            fixture.pid,
            &fixture.socket_path,
            context.nonce,
            &callers,
        )
        .is_ok_and(|rejoined| rejoined == joined);
    let (topology_readable, current_owned_space, topology_row_exact) =
        owned_topology_facts(leg.transport_mut(), owned);
    let windows_on_owned_space = await_joined_on_owned_space(joined, owned.sid()).is_ok();
    let exact = record_exact
        && product_live_exact
        && fixture_identity_readable
        && fixture_bundle_match
        && fixture_exe_match
        && fixture_launch_match
        && fixture_nonce_match
        && fixture_join_same
        && topology_readable
        && current_owned_space
        && topology_row_exact
        && windows_on_owned_space;
    let evidence = serde_json::json!({
        "product_record_exact": record_exact,
        "product_pid": launch2_pid,
        "product_live_identity_exact": product_live_exact,
        "fixture_identity_readable": fixture_identity_readable,
        "fixture_bundle_match": fixture_bundle_match,
        "fixture_exe_match": fixture_exe_match,
        "fixture_launch_match": fixture_launch_match,
        "fixture_nonce_match": fixture_nonce_match,
        "fixture_join_same": fixture_join_same,
        "topology_readable": topology_readable,
        "current_owned_space": current_owned_space,
        "topology_row_exact": topology_row_exact,
        "windows_on_owned_space": windows_on_owned_space,
    });
    if !exact {
        return Err((
            1,
            serde_json::json!({
                "stage": "post-restart-capture",
                "error": "the post-restart capture is not bound to the exact product, fixture, owned Space and window topology; refusing",
                "capture": evidence,
            })
            .to_string(),
        ));
    }
    Ok(evidence)
}

/// One authenticated fixture instance plus its joined windows and the one
/// owned Space they were born on. Grouped so the guard, nudge and capture
/// checks all read the same bound facts rather than parallel arguments.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
pub(crate) struct OwnedRestartFixture<'a> {
    pub(crate) fixture: &'a FixtureRecord,
    pub(crate) nonce: &'a str,
    pub(crate) joined: &'a [(String, u32)],
    pub(crate) owned: &'a PostOnlyOwnedSpace,
}

/// Read every guard fact at one full checkpoint: before the one nudge and
/// again before the second launch. No effect except read-only socket/native
/// reads; an unreadable fact is recorded as an uncertainty for the pure
/// verdict, never defaulted to success.
#[cfg(target_os = "macos")]
pub(crate) fn observe_restart_guard(
    leg: &mut LiveLeg<LiveLegTransport>,
    context: OwnedRestartFixture<'_>,
    prior: &RestartStopEvidence,
    store_dir: &Path,
    expected_store: &StoreSnapshot,
    launch_ids_distinct: bool,
) -> Result<RestartGuardInputs, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    let owned = context.owned;
    let store_observed = read_store_snapshot(store_dir).map_err(|error| (1, error))?;
    let (product_overlap, product_overlap_note) = match foreign_product_overlap(leg, fixture.pid) {
        Ok(overlap) => (overlap, None),
        Err(error) => (true, Some(error)),
    };
    let prior_liveness = match leg.transport_mut().process_gone(prior.pid) {
        Ok(true) => Liveness::Gone,
        Ok(false) => Liveness::Present,
        Err(error) => Liveness::Unreadable(error),
    };
    let fixture_liveness = match leg.transport_mut().process_gone(fixture.pid) {
        Ok(true) => Liveness::Gone,
        Ok(false) => Liveness::Present,
        Err(error) => Liveness::Unreadable(error),
    };
    let pid = i32::try_from(prior.pid).unwrap_or(0);
    let identity = crate::ffi::ns_app_identity_for_pid(pid);
    let prior_lifetime_gone = !(identity.exe.as_deref() == Some(prior.exe.as_str())
        && identity.launch_date.as_deref() == Some(prior.launch.as_str()));
    let identity = crate::fixture_post::fixture_process_identity(fixture.pid);
    let (fixture_identity_readable, fixture_bundle_match, fixture_exe_match, fixture_launch_match) =
        match identity {
            Ok(identity) => (
                true,
                identity.bundle.as_deref() == Some(FIXTURE_SCOPE),
                identity.exe.as_deref() == Some(fixture.exe.as_str()),
                identity.launch.as_deref() == Some(fixture.launch.as_str()),
            ),
            Err(_) => (false, false, false, false),
        };
    let fixture_nonce_match =
        read_run_nonce(&fixture.run_dir).is_ok_and(|live| live == context.nonce);
    let callers: Vec<String> = joined.iter().map(|(caller, _)| caller.clone()).collect();
    let (fixture_join_same, fixture_join_note) =
        match join_fixture_windows(
            fixture.pid,
            &fixture.socket_path,
            context.nonce,
            &callers,
        ) {
            Ok(rejoined) => (rejoined == joined, None),
            Err(error) => (false, Some(error)),
        };
    let (topology_readable, current_owned_space, topology_row_exact) =
        owned_topology_facts(leg.transport_mut(), owned);
    let windows_on_owned_space = await_joined_on_owned_space(joined, owned.sid()).is_ok();
    Ok(RestartGuardInputs {
        launch_ids_distinct,
        product_overlap,
        product_overlap_note,
        prior_terminal_clean: clean_terminal_verdict(&prior.outcome).is_ok(),
        prior_liveness,
        prior_lifetime_gone,
        fixture_liveness,
        fixture_identity_readable,
        fixture_bundle_match,
        fixture_exe_match,
        fixture_launch_match,
        fixture_nonce_match,
        fixture_join_same,
        fixture_join_note,
        topology_readable,
        current_owned_space,
        topology_row_exact,
        windows_on_owned_space,
        store_expected: expected_store.clone(),
        store_observed,
    })
}
