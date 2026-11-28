#![forbid(unsafe_code)]

//! Development-only fixture-owned Space gate (macOS adapter boundary).
//!
//! The leg controller creates run-owned disposable native Spaces and passes
//! their exact SIDs to the fixture-scoped product through the repeatable
//! `--fixture-owned-space SID` flag (plus `--fixture-starting-space SID`
//! for the recorded initial current). The product enforces this gate at the
//! actual native touch in its adapter backends, on the reply-resolved SID:
//! destinations outside the owned set refuse with the existing truthful
//! pre-dispatch `Refused` (genuine pre/post reads staged verbatim, never a
//! faked success), creations refuse while gated (the controller owns
//! create/remove), and releases always attempt (never blocked).
//!
//! No core, protocol, or shared policy change: this type lives in the macOS
//! adapter and is stored in adapter request/controller constructors only.
//! Absent (no flags) means disabled: the normal product path is unchanged.
//! An explicitly requested but unsafe configuration (wrong scope, unscoped,
//! no harness run directory, empty or zero SIDs) refuses at validation,
//! never silently expanding authority.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Fixture-controller detached fullscreen registration leaf under the
/// product run directory. Written by the leg controller before the first
/// gated select, read by the product at the exact native dispatch.
pub const DETACHED_REGISTRATION_FILE: &str = "detached-registration.json";

/// Bounded registration body: the exact fixed fields only.
const DETACHED_REGISTRATION_MAX_BYTES: u64 = 1024;

/// Exact fixture detached fullscreen surface/member registration: the
/// controller-proven fullscreen Space SID, the member window identity, and
/// the pinned output key. Plain IDs only; no titles, paths, or user content.
/// Unknown fields refuse so the cross-process proof is exact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureDetachedRegistration {
    pub surface: u64,
    pub member: u32,
    pub pid: u32,
    pub output: String,
}

impl FixtureDetachedRegistration {
    /// Validate the shaped registration before publish or after read.
    pub fn validate(&self) -> Result<(), String> {
        if self.surface == 0 {
            return Err("detached registration: zero surface SID".to_owned());
        }
        if self.member == 0 {
            return Err("detached registration: zero member window id".to_owned());
        }
        if self.pid <= 1 {
            return Err("detached registration: invalid member pid".to_owned());
        }
        if self.output.is_empty() {
            return Err("detached registration: empty output key".to_owned());
        }
        Ok(())
    }

    /// Publish the exact registration once into the private product run
    /// directory. The leaf is created new (never overwritten) and bounded;
    /// every check precedes the single publication, and the product reads it
    /// only after this returns.
    pub fn publish(&self, run_dir: &Path) -> Result<(), String> {
        self.validate()?;
        validate_private_run_dir(run_dir)?;
        let body = serde_json::to_vec(self)
            .map_err(|e| format!("detached registration encode refused ({e})"))?;
        if body.len() as u64 > DETACHED_REGISTRATION_MAX_BYTES {
            return Err("detached registration body exceeds its bound; refusing".to_owned());
        }
        let path = run_dir.join(DETACHED_REGISTRATION_FILE);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("detached registration write refused ({e})"))?;
        file.write_all(&body)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("detached registration write refused ({e})"))
    }

    /// Read the registration if present. Refuses a symlink, a non-regular or
    /// oversized leaf, unknown or malformed fields, and any run directory
    /// that is not the validated private actual one; absent stays `None`.
    pub fn read(run_dir: &Path) -> Result<Option<Self>, String> {
        validate_private_run_dir(run_dir)?;
        let path = run_dir.join(DETACHED_REGISTRATION_FILE);
        if registration_leaf_metadata(&path)?.is_none() {
            return Ok(None);
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options
            .open(&path)
            .map_err(|e| format!("detached registration read refused ({e})"))?;
        let meta = file
            .metadata()
            .map_err(|e| format!("detached registration read refused ({e})"))?;
        if !meta.is_file() || meta.len() > DETACHED_REGISTRATION_MAX_BYTES {
            return Err(
                "detached registration leaf is not a bounded regular file; refusing".to_owned(),
            );
        }
        let mut raw = Vec::with_capacity(meta.len() as usize + 1);
        file.take(DETACHED_REGISTRATION_MAX_BYTES + 1)
            .read_to_end(&mut raw)
            .map_err(|e| format!("detached registration read refused ({e})"))?;
        if raw.len() as u64 > DETACHED_REGISTRATION_MAX_BYTES {
            return Err("detached registration body exceeds its bound; refusing".to_owned());
        }
        let parsed: Self = serde_json::from_slice(&raw)
            .map_err(|e| format!("detached registration parse refused ({e})"))?;
        parsed.validate()?;
        Ok(Some(parsed))
    }
}

/// Exact private run-directory proof: the actual directory the validated
/// fixture gate owns, never a symlink or a shared/public location.
fn validate_private_run_dir(run_dir: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(run_dir)
        .map_err(|e| format!("detached registration: run directory unreadable ({e})"))?;
    if meta.file_type().is_symlink() {
        return Err("detached registration: run directory is a symlink; refusing".to_owned());
    }
    if !meta.is_dir() {
        return Err("detached registration: run directory is not a directory; refusing".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o777 != 0o700 {
            return Err("detached registration: run directory is not private; refusing".to_owned());
        }
        if meta.uid() != crate::ffi::current_uid() {
            return Err(
                "detached registration: run directory is not owned by the current UID; refusing"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

/// Bounded regular-leaf proof for the registration path: symlinks, other
/// file types, and oversized bodies refuse; absence is the only `None`.
fn registration_leaf_metadata(path: &Path) -> Result<Option<std::fs::Metadata>, String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err("detached registration leaf is a symlink; refusing".to_owned())
        }
        Ok(meta) if !meta.is_file() => {
            Err("detached registration leaf is not a regular file; refusing".to_owned())
        }
        Ok(meta) if meta.len() > DETACHED_REGISTRATION_MAX_BYTES => {
            Err("detached registration body exceeds its bound; refusing".to_owned())
        }
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("detached registration read refused ({e})")),
    }
}

/// Exact fixture bundle scope admitted with a gate. No other scope, never
/// unscoped.
pub const FIXTURE_TEST_SCOPE: &str = "io.github.omnitiler.live-fixture";

/// Fixture-controller-only one-shot ordinary float issuing-reply hold.
///
/// The controller launches the fixture-scoped product with this environment
/// variable naming the exact rect-less `ToggleFloat` occurrence to hold
/// (`1`-based, counted only for applied ordinary chord replies). No path, no
/// file, no deletion. The product honors it only when the fixture gate is
/// present (exact fixture scope, harness run directory, owned Space SIDs) and
/// suppresses that one reply's emission in memory. Absent leaves the normal
/// product path unchanged.
pub const REPLY_HOLD_ENV: &str = "OMNITILER_FIXTURE_HOLD_FLOAT_REPLY";

/// In-memory, fixture-gated one-shot issuance hold. Constructed only with a
/// validated [`FixtureSpaceGate`] and the exact fixture scope, so consumption
/// re-checks both the stored scope and the non-empty owned-Space gate.
#[derive(Debug)]
pub struct FixtureReplyHold {
    scope: String,
    gate: FixtureSpaceGate,
    occurrence: u32,
    seen: AtomicU32,
    held: AtomicBool,
}

impl FixtureReplyHold {
    /// Read the controller's occurrence from the launch environment. Absent
    /// is inert; present without the exact fixture gate refuses instead of
    /// silently not holding.
    pub(crate) fn from_env(
        gate: Option<&FixtureSpaceGate>,
        scopes: &[String],
    ) -> Result<Option<Arc<Self>>, String> {
        let Some(raw) = std::env::var_os(REPLY_HOLD_ENV) else {
            return Ok(None);
        };
        let Some(gate) = gate else {
            return Err(format!(
                "tile: {REPLY_HOLD_ENV} needs --fixture-owned-space (exact fixture scope, run dir, owned Space)"
            ));
        };
        let raw = raw
            .to_str()
            .ok_or_else(|| format!("tile: {REPLY_HOLD_ENV} must be an ASCII occurrence"))?;
        let occurrence: u32 = raw.parse().map_err(|_| {
            format!("tile: {REPLY_HOLD_ENV} must be a 1-based occurrence, got '{raw}'")
        })?;
        Self::arm(gate, scopes, occurrence).map(Some)
    }

    /// Arm one exact occurrence behind the validated fixture gate and exact
    /// fixture scope. Crate-internal so tests exercise the same construction;
    /// no production caller can arm without a gate.
    pub(crate) fn arm(
        gate: &FixtureSpaceGate,
        scopes: &[String],
        occurrence: u32,
    ) -> Result<Arc<Self>, String> {
        if scopes.len() != 1 || scopes[0] != FIXTURE_TEST_SCOPE {
            return Err(format!(
                "tile: {REPLY_HOLD_ENV} needs exactly --scope {FIXTURE_TEST_SCOPE}"
            ));
        }
        if occurrence == 0 || occurrence > 64 {
            return Err(format!(
                "tile: {REPLY_HOLD_ENV} occurrence must be 1..=64, got {occurrence}"
            ));
        }
        Ok(Arc::new(Self {
            scope: FIXTURE_TEST_SCOPE.to_owned(),
            gate: gate.clone(),
            occurrence,
            seen: AtomicU32::new(0),
            held: AtomicBool::new(false),
        }))
    }

    /// Claim the configured ordinary rect-less float occurrence. The caller
    /// matches the command first; this re-checks the exact fixture scope and
    /// a non-empty owned-Space gate, counts only callers that pass, and
    /// returns true exactly once for the configured occurrence.
    pub(crate) fn claim_ordinary_float(&self) -> bool {
        if self.scope != FIXTURE_TEST_SCOPE || self.gate.owned().is_empty() {
            return false;
        }
        let seen = self.seen.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        if seen != self.occurrence {
            return false;
        }
        !self.held.swap(true, Ordering::SeqCst)
    }

    /// Console evidence for the exact held occurrence.
    pub(crate) const fn occurrence(&self) -> u32 {
        self.occurrence
    }
}

/// Opaque native-SID allowlist for one fixture-gated product run: the exact
/// SIDs the leg controller created plus the recorded initial starting
/// current (the stay-return source). Opaque: membership only, no policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FixtureSpaceGate {
    allowed: BTreeSet<u64>,
    starting: Option<u64>,
    run_dir: Option<String>,
}

impl FixtureSpaceGate {
    /// Build the gate from controller-published SIDs. Refuses an empty set
    /// (absent/empty must never silently widen authority) and zero SIDs.
    pub fn new(owned: &[u64], starting: Option<u64>) -> Result<Self, String> {
        if owned.is_empty() {
            return Err(
                "fixture gate refused: empty owned-Space set (absent/empty never widens authority)"
                    .to_owned(),
            );
        }
        let mut allowed = BTreeSet::new();
        for sid in owned {
            if *sid == 0 {
                return Err("fixture gate refused: zero SID unownable".to_owned());
            }
            allowed.insert(*sid);
        }
        if let Some(start) = starting
            && start == 0
        {
            return Err("fixture gate refused: zero starting SID".to_owned());
        }
        Ok(Self {
            allowed,
            starting,
            run_dir: None,
        })
    }

    /// Attach the validated harness run directory (never a caller-supplied
    /// arbitrary path: only [`validate_fixture_gate`] calls this).
    fn with_run_dir(mut self, run_dir: &str) -> Self {
        self.run_dir = Some(run_dir.to_owned());
        self
    }

    /// Validated product run directory, if this gate was built by
    /// [`validate_fixture_gate`].
    #[must_use]
    pub fn run_dir(&self) -> Option<&str> {
        self.run_dir.as_deref()
    }

    /// Controller-published detached registration for this exact run, if
    /// present. A gate without a validated run directory returns `None`
    /// (unit-constructed gates never read arbitrary paths).
    pub fn detached_registration(&self) -> Result<Option<FixtureDetachedRegistration>, String> {
        let Some(dir) = self.run_dir.as_deref() else {
            return Ok(None);
        };
        FixtureDetachedRegistration::read(Path::new(dir))
    }

    /// Owned destinations (select/send/bridged targets, trailing reuse):
    /// exactly the controller-created SIDs.
    #[must_use]
    pub fn allows(&self, sid: u64) -> bool {
        sid != 0 && self.allowed.contains(&sid)
    }

    /// Stay-return sources: the owned set plus the recorded initial
    /// starting current (the controller-owned restore target).
    #[must_use]
    pub fn allows_or_starting(&self, sid: u64) -> bool {
        self.allows(sid) || Some(sid) == self.starting.filter(|s| *s != 0)
    }

    /// Owned SIDs in order (controller evidence only).
    #[must_use]
    pub fn owned(&self) -> Vec<u64> {
        self.allowed.iter().copied().collect()
    }

    /// Recorded initial starting current, if any.
    #[must_use]
    pub fn starting(&self) -> Option<u64> {
        self.starting
    }
}

/// Validate one tile request's gate configuration. Returns `None` (disabled,
/// normal product unchanged) when no gate flags were passed; otherwise
/// builds the gate, refusing unsafe test configuration with a usage error:
/// the exact fixture scope only, never unscoped, and only with an explicit
/// harness run directory. Pure; unit-tested.
pub fn validate_fixture_gate(
    scopes: &[String],
    unscoped: bool,
    run_dir: &Option<String>,
    owned: &[u64],
    starting: Option<u64>,
) -> Result<Option<FixtureSpaceGate>, String> {
    if owned.is_empty() && starting.is_none() {
        return Ok(None);
    }
    if unscoped {
        return Err(
            "tile: --fixture-owned-space refuses --unscoped (fixture scope only)".to_owned(),
        );
    }
    if scopes.len() != 1 || scopes[0] != FIXTURE_TEST_SCOPE {
        return Err(format!(
            "tile: --fixture-owned-space needs exactly --scope {FIXTURE_TEST_SCOPE}"
        ));
    }
    let dir = match run_dir {
        Some(dir) if !dir.is_empty() => dir.as_str(),
        _ => {
            return Err(
                "tile: --fixture-owned-space needs --run-dir (explicit harness context)".to_owned(),
            );
        }
    };
    FixtureSpaceGate::new(owned, starting)
        .map(|gate| gate.with_run_dir(dir))
        .map(Some)
}

/// Parse one `--fixture-owned-space SID` / `--fixture-starting-space SID`
/// value: nonzero native SID. Pure.
pub fn parse_gate_sid(raw: &str, flag: &str) -> Result<u64, String> {
    let sid: u64 = raw
        .parse()
        .map_err(|_| format!("{flag} must be a nonzero SID, got '{raw}'"))?;
    if sid == 0 {
        return Err(format!("{flag} must be a nonzero SID, got '{raw}'"));
    }
    Ok(sid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_gate_leaves_normal_product_unchanged() {
        let gate =
            validate_fixture_gate(&["com.apple.TextEdit".to_owned()], false, &None, &[], None)
                .expect("absent gate validates");
        assert_eq!(gate, None);
    }

    #[test]
    fn gate_needs_exact_fixture_scope_unscoped_denied_and_run_dir() {
        let owned = vec![101, 102];
        // Unscoped refuses.
        assert!(
            validate_fixture_gate(&[], true, &Some("/tmp/run".to_owned()), &owned, Some(11))
                .is_err()
        );
        // Wrong scope refuses.
        assert!(
            validate_fixture_gate(
                &["com.apple.TextEdit".to_owned()],
                false,
                &Some("/tmp/run".to_owned()),
                &owned,
                Some(11)
            )
            .is_err()
        );
        // Missing run dir refuses.
        assert!(
            validate_fixture_gate(
                &[FIXTURE_TEST_SCOPE.to_owned()],
                false,
                &None,
                &owned,
                Some(11)
            )
            .is_err()
        );
        // Empty owned set with no starting is disabled, not a gate.
        let gate = validate_fixture_gate(
            &[FIXTURE_TEST_SCOPE.to_owned()],
            false,
            &Some("/tmp/run".to_owned()),
            &[],
            None,
        )
        .expect("empty is disabled");
        assert_eq!(gate, None);
        // Zero SID refuses.
        assert!(
            validate_fixture_gate(
                &[FIXTURE_TEST_SCOPE.to_owned()],
                false,
                &Some("/tmp/run".to_owned()),
                &[0],
                None
            )
            .is_err()
        );
        // Exact configuration builds.
        let gate = validate_fixture_gate(
            &[FIXTURE_TEST_SCOPE.to_owned()],
            false,
            &Some("/tmp/run".to_owned()),
            &owned,
            Some(11),
        )
        .expect("exact gate builds")
        .expect("gate present");
        assert!(gate.allows(101));
        assert!(gate.allows(102));
        assert!(!gate.allows(11));
        assert!(!gate.allows(999));
        assert!(gate.allows_or_starting(11));
        assert!(gate.allows_or_starting(101));
        assert!(!gate.allows_or_starting(999));
        assert!(!gate.allows(0));
    }

    #[test]
    fn starting_only_still_needs_safe_configuration() {
        // Starting without owned refuses (empty set never widens).
        assert!(FixtureSpaceGate::new(&[], Some(11)).is_err());
        assert!(parse_gate_sid("0", "--fixture-owned-space").is_err());
        assert_eq!(parse_gate_sid("42", "--fixture-owned-space"), Ok(42));
    }

    #[test]
    fn detached_registration_round_trips_only_through_the_private_actual_run_dir() {
        let dir = private_test_dir("detached-round-trip");
        let gate = validate_fixture_gate(
            &[FIXTURE_TEST_SCOPE.to_owned()],
            false,
            &Some(dir.to_string_lossy().into_owned()),
            &[101],
            None,
        )
        .expect("gate validates")
        .expect("gate present");
        assert!(gate.detached_registration().expect("absent reads").is_none());
        let registration = FixtureDetachedRegistration {
            surface: 320,
            member: 10747,
            pid: 12168,
            output: "macos-display-1".to_owned(),
        };
        registration.publish(&dir).expect("registration writes");
        assert_eq!(
            gate.detached_registration().expect("registration reads"),
            Some(registration.clone())
        );
        // Publication is create-new: a second publish never overwrites.
        assert!(registration.publish(&dir).is_err());
        // A malformed existing leaf refuses instead of being treated absent.
        std::fs::write(dir.join(DETACHED_REGISTRATION_FILE), b"{}").expect("bad leaf writes");
        assert!(gate.detached_registration().is_err());
        // Unknown fields are not the exact registration shape.
        std::fs::write(
            dir.join(DETACHED_REGISTRATION_FILE),
            br#"{"surface":320,"member":10747,"pid":12168,"output":"macos-display-1","extra":1}"#,
        )
        .expect("extra-field leaf writes");
        assert!(gate.detached_registration().is_err());
        // An oversized body refuses before it is parsed.
        std::fs::write(
            dir.join(DETACHED_REGISTRATION_FILE),
            vec![b' '; (DETACHED_REGISTRATION_MAX_BYTES + 1) as usize],
        )
        .expect("oversized leaf writes");
        assert!(gate.detached_registration().is_err());
        // A symlink leaf is never followed, even to the valid body.
        let valid = private_test_dir("detached-valid");
        registration.publish(&valid).expect("valid registration writes");
        let leaf = dir.join(DETACHED_REGISTRATION_FILE);
        std::fs::remove_file(&leaf).expect("test leaf removes");
        #[cfg(unix)]
        std::os::unix::fs::symlink(valid.join(DETACHED_REGISTRATION_FILE), &leaf)
            .expect("leaf symlink creates");
        #[cfg(unix)]
        assert!(gate.detached_registration().is_err());
        // A symlinked or shared run directory refuses too.
        #[cfg(unix)]
        {
            let link = std::env::temp_dir().join(format!(
                "fixture-gate-detached-link-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&valid, &link).expect("dir symlink creates");
            assert!(FixtureDetachedRegistration::read(&link).is_err());
            let _ = std::fs::remove_file(&link);
        }
        #[cfg(unix)]
        {
            std::fs::set_permissions(&valid, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .expect("test permissions set");
            assert!(FixtureDetachedRegistration::read(&valid).is_err());
        }
        // A unit-constructed gate never reads an arbitrary path.
        assert!(
            FixtureSpaceGate::new(&[101], None)
                .expect("unit gate")
                .detached_registration()
                .expect("no dir reads absent")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&valid);
    }

    fn private_test_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fixture-gate-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("run dir creates");
        #[cfg(unix)]
        std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .expect("private run dir mode");
        dir
    }
}
