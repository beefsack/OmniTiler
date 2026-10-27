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

/// Exact fixture bundle scope admitted with a gate. No other scope, never
/// unscoped.
pub const FIXTURE_TEST_SCOPE: &str = "io.github.omnitiler.live-fixture";

/// Opaque native-SID allowlist for one fixture-gated product run: the exact
/// SIDs the leg controller created plus the recorded initial starting
/// current (the stay-return source). Opaque: membership only, no policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FixtureSpaceGate {
    allowed: BTreeSet<u64>,
    starting: Option<u64>,
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
        Ok(Self { allowed, starting })
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
    match run_dir {
        Some(dir) if !dir.is_empty() => {}
        _ => {
            return Err(
                "tile: --fixture-owned-space needs --run-dir (explicit harness context)".to_owned(),
            );
        }
    }
    FixtureSpaceGate::new(owned, starting).map(Some)
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
}
