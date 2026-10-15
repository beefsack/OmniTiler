#![forbid(unsafe_code)]

//! Scoped-tile app identity: explicit allowlist plus PID-reuse binding.
//!
//! Pure, portable, no live state, no titles. The runtime consults this at
//! admission (config validation) AND at the last native effect boundary
//! (before every AX setter and focus dispatch); either refusal means no
//! write and no focus. Unknown or out-of-scope targets never actuate.
//!
//! - Ghostty is rejected case-insensitively (including bundle-id variants
//!   such as `com.mitchellh.ghostty`) at validation and at the effect
//!   boundary. The existing [`crate::ownership::is_protected_bundle`] gate
//!   already covers it; [`is_ghostty_bundle`] pins the explicit product rule
//!   here so the scope does not depend on that list's wording.
//! - The crate's own probe bundle ([`crate::BUNDLE_ID`]) is rejected from
//!   tile scopes: own windows are never observed (see [`crate::classifier`]).
//!   Directly spawned probe-target helpers carry NO LaunchServices bundle
//!   entry, so they never match a bundle allowlist entry; the native tile
//!   backend additionally requires a live launch date, which direct-spawn
//!   helpers lack. The signed bundled helper ([`crate::HELPER_BUNDLE_ID`],
//!   packaged by `scripts/macos-app.sh`) DOES carry a LaunchServices entry
//!   plus launch date, so an explicit helper-bundle scope admits it through
//!   [`AppScope::allows`] exactly like any named app. There is intentionally
//!   no other helper bundle ID to allowlist: a `None` bundle never
//!   authorizes a tile write through [`AppScope::allows`].
//! - PID reuse cannot authorize a new process: [`ProcessIdentity`] binds the
//!   launch lifetime to bundle plus executable path plus a start token
//!   (live `launchDate` string where available, else the spawn nonce), and
//!   [`ProcessIdentity::revalidate`] demands exact equality before effects.
//!   TOCTOU cannot be made atomic; these are honest narrow checks, not a
//!   guarantee of impossibility.

use serde::{Deserialize, Serialize};

/// Case-insensitive Ghostty marker. Matches the bundle ID and its variants
/// (`com.mitchellh.ghostty`, `com.mitchellh.ghostty-beta`, etc.).
pub const GHOSTTY_MARKER: &str = "ghostty";

/// True when `bundle_id` names Ghostty in any letter case.
#[must_use]
pub fn is_ghostty_bundle(bundle_id: &str) -> bool {
    bundle_id.to_ascii_lowercase().contains(GHOSTTY_MARKER)
}

/// True when `bundle_id` is refused at the effect boundary: Ghostty
/// (explicit product rule), the crate's own probe bundle (never observe
/// self), or any protected controlling-terminal bundle from the shared
/// ownership gate.
#[must_use]
pub fn is_effect_refused_bundle(bundle_id: &str) -> bool {
    if bundle_id.is_empty() {
        return true;
    }
    if is_ghostty_bundle(bundle_id) {
        return true;
    }
    if bundle_id == crate::BUNDLE_ID {
        return true;
    }
    crate::ownership::is_protected_bundle(bundle_id)
}

/// Explicit bundle-ID allowlist for one scoped-tile session. Empty means
/// scope enforcement is disabled (legacy synthetic fixtures); non-empty
/// means only listed bundles may actuate, and every other bundle —
/// including unknown (`None`) — never writes or focuses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppScope {
    pub allowed: Vec<String>,
}

impl AppScope {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            allowed: Vec::new(),
        }
    }

    #[must_use]
    pub fn enforced(&self) -> bool {
        !self.allowed.is_empty()
    }

    /// Validate the allowlist itself. Rejects Ghostty (any case/variant)
    /// and the own probe bundle: neither may appear in a tile scope.
    /// The bundled helper bundle ([`crate::HELPER_BUNDLE_ID`]) IS admittable
    /// when listed explicitly (it carries a LaunchServices entry plus launch
    /// date, unlike direct-spawn helpers). Empty entries are rejected.
    /// Protected-terminal bundles beyond Ghostty
    /// are NOT rejected here (they may be listed deliberately in a dev
    /// session); the effect boundary still refuses them unless the caller
    /// opts out explicitly — see [`allows_effect`].
    pub fn validate(&self) -> Result<(), String> {
        for bundle in &self.allowed {
            if bundle.is_empty() {
                return Err("scope allowlist must not contain empty bundle IDs".to_owned());
            }
            if is_ghostty_bundle(bundle) {
                return Err(format!(
                    "scope allowlist must not contain Ghostty '{bundle}'"
                ));
            }
            if bundle == crate::BUNDLE_ID {
                return Err(format!(
                    "scope allowlist must not contain the own probe bundle '{}'",
                    crate::BUNDLE_ID
                ));
            }
        }
        Ok(())
    }

    /// Admission check for a live bundle observation. `None` (unreadable
    /// bundle) never authorizes. Ghostty and the own bundle never authorize
    /// even when listed (defense in depth: validation should already have
    /// refused them). Disabled scopes (empty allowlist) authorize nothing
    /// here — legacy fixtures bypass scope entirely via the unenforced path
    /// instead of calling this.
    #[must_use]
    pub fn allows(&self, bundle: Option<&str>) -> bool {
        match bundle {
            Some(id) => {
                if is_ghostty_bundle(id) || id == crate::BUNDLE_ID {
                    return false;
                }
                self.allowed.iter().any(|a| a == id)
            }
            None => false,
        }
    }

    /// Last-native-effect-boundary check. Like [`allows`] plus the shared
    /// protected-terminal gate: a listed terminal bundle still refuses the
    /// actual write/focus. Unknown (`None`) never authorizes.
    #[must_use]
    pub fn allows_effect(&self, bundle: Option<&str>) -> bool {
        match bundle {
            Some(id) => {
                if is_effect_refused_bundle(id) {
                    return false;
                }
                self.allowed.iter().any(|a| a == id)
            }
            None => false,
        }
    }
}

/// Launch-lifetime binding for one observed process: bundle plus executable
/// path plus a per-run start token. The start token is the live
/// `launchDate` string for LaunchServices apps where available, else the
/// probe spawn nonce for directly spawned helpers (which have no bundle
/// entry). Exact equality on all three (plus PID) is required by
/// [`revalidate`]; a PID reuse with a different executable or start token
/// is refused instead of authorized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub bundle: Option<String>,
    pub exe: String,
    pub start_token: String,
}

impl ProcessIdentity {
    pub fn bind(
        pid: u32,
        bundle: Option<&str>,
        exe: &str,
        start_token: &str,
    ) -> Result<Self, String> {
        Self::bind_inner(pid, bundle, exe, start_token, false)
    }

    /// Unscoped product binding: identical lifetime checks to [`bind`], but
    /// Ghostty binds as an ordinary candidate (only the scoped allowlist
    /// mode refuses it). Scoped callers must keep using [`bind`].
    pub fn bind_unscoped(
        pid: u32,
        bundle: Option<&str>,
        exe: &str,
        start_token: &str,
    ) -> Result<Self, String> {
        Self::bind_inner(pid, bundle, exe, start_token, true)
    }

    fn bind_inner(
        pid: u32,
        bundle: Option<&str>,
        exe: &str,
        start_token: &str,
        ghostty_ok: bool,
    ) -> Result<Self, String> {
        if pid <= 1 {
            return Err(format!("refusing identity for reserved pid {pid}"));
        }
        if exe.is_empty() {
            return Err("refusing identity without an executable path".to_owned());
        }
        if start_token.is_empty() {
            return Err("refusing identity without a start token".to_owned());
        }
        if !ghostty_ok
            && let Some(id) = bundle
            && is_ghostty_bundle(id)
        {
            return Err("refusing Ghostty identity".to_owned());
        }
        Ok(Self {
            pid,
            bundle: bundle.map(str::to_owned),
            exe: exe.to_owned(),
            start_token: start_token.to_owned(),
        })
    }

    /// Revalidate live observations against the bound lifetime. Any drift —
    /// PID, bundle, executable, or start token — refuses (possible PID
    /// reuse). Narrow check only; not an atomicity guarantee.
    pub fn revalidate(
        &self,
        live_pid: u32,
        live_bundle: Option<&str>,
        live_exe: &str,
        live_start: &str,
    ) -> Result<(), String> {
        self.revalidate_inner(live_pid, live_bundle, live_exe, live_start, false)
    }

    /// Unscoped product revalidation: identical drift checks to
    /// [`revalidate`], but Ghostty passes as an ordinary candidate.
    /// Scoped callers must keep using [`revalidate`].
    pub fn revalidate_unscoped(
        &self,
        live_pid: u32,
        live_bundle: Option<&str>,
        live_exe: &str,
        live_start: &str,
    ) -> Result<(), String> {
        self.revalidate_inner(live_pid, live_bundle, live_exe, live_start, true)
    }

    fn revalidate_inner(
        &self,
        live_pid: u32,
        live_bundle: Option<&str>,
        live_exe: &str,
        live_start: &str,
        ghostty_ok: bool,
    ) -> Result<(), String> {
        if live_pid != self.pid {
            return Err(format!(
                "identity pid drift (bound {}, live {live_pid}); refusing",
                self.pid
            ));
        }
        if self.bundle.as_deref() != live_bundle {
            return Err("identity bundle drift (possible PID reuse); refusing".to_owned());
        }
        if self.exe != live_exe {
            return Err("identity executable drift (possible PID reuse); refusing".to_owned());
        }
        if self.start_token != live_start {
            return Err("identity start-token drift (possible PID reuse); refusing".to_owned());
        }
        if !ghostty_ok
            && let Some(id) = live_bundle
            && is_ghostty_bundle(id)
        {
            return Err("refusing Ghostty identity at the effect boundary".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghostty_matches_case_insensitively_including_variants() {
        for id in [
            "com.mitchellh.ghostty",
            "COM.MITCHELLH.GHOSTTY",
            "com.mitchellh.ghostty-beta",
            "io.Ghostty.Dev",
        ] {
            assert!(is_ghostty_bundle(id), "{id}");
            assert!(is_effect_refused_bundle(id), "{id}");
        }
        assert!(!is_ghostty_bundle("com.example.editor"));
        assert!(!is_ghostty_bundle(crate::BUNDLE_ID));
    }

    #[test]
    fn own_bundle_refused_at_boundary() {
        assert!(is_effect_refused_bundle(crate::BUNDLE_ID));
        let scope = AppScope {
            allowed: vec!["com.example.app".to_owned()],
        };
        assert!(!scope.allows(Some(crate::BUNDLE_ID)));
        assert!(!scope.allows_effect(Some(crate::BUNDLE_ID)));
    }

    #[test]
    fn validation_rejects_ghostty_and_own_bundle_in_allowlist() {
        let ghostty = AppScope {
            allowed: vec!["com.mitchellh.GhostTy".to_owned()],
        };
        assert!(ghostty.validate().is_err());
        let own = AppScope {
            allowed: vec![crate::BUNDLE_ID.to_owned()],
        };
        assert!(own.validate().is_err());
        let empty = AppScope {
            allowed: vec![String::new()],
        };
        assert!(empty.validate().is_err());
        let ok = AppScope {
            allowed: vec!["com.example.app".to_owned()],
        };
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn unknown_bundle_never_authorizes() {
        let scope = AppScope {
            allowed: vec!["com.example.app".to_owned()],
        };
        assert!(!scope.allows(None));
        assert!(!scope.allows_effect(None));
        assert!(!scope.allows(Some("com.example.other")));
        assert!(scope.allows(Some("com.example.app")));
        assert!(scope.allows_effect(Some("com.example.app")));
    }

    #[test]
    fn helper_without_bundle_never_matches_allowlist() {
        // Directly spawned helpers have no bundle entry: `None` never
        // authorizes a tile write; they bind via `ProcessIdentity` instead.
        let scope = AppScope {
            allowed: vec!["com.example.app".to_owned()],
        };
        assert!(!scope.allows(None));
        let identity =
            ProcessIdentity::bind(4242, None, "/tmp/tiler-macos-probe-target", "nonce-token")
                .expect("helper binds without a bundle");
        assert_eq!(identity.bundle, None);
        assert!(
            identity
                .revalidate(4242, None, "/tmp/tiler-macos-probe-target", "nonce-token")
                .is_ok()
        );
    }

    #[test]
    fn bundled_helper_bundle_admits_when_listed_explicitly() {
        // The signed bundled helper is an ordinary admittable scope member:
        // listed explicitly it validates, admits, and actuates; unlisted it
        // refuses like any other out-of-scope bundle. The own probe bundle
        // stays refused in the same positions.
        let helper = crate::HELPER_BUNDLE_ID;
        assert!(!is_effect_refused_bundle(helper));
        let scope = AppScope {
            allowed: vec![helper.to_owned()],
        };
        assert!(scope.validate().is_ok());
        assert!(scope.allows(Some(helper)));
        assert!(scope.allows_effect(Some(helper)));
        assert!(!scope.allows(Some("com.example.other")));
        assert!(!scope.allows_effect(Some("com.example.other")));
        let unlisted = AppScope {
            allowed: vec!["com.example.app".to_owned()],
        };
        assert!(!unlisted.allows(Some(helper)));
        assert!(!unlisted.allows_effect(Some(helper)));
    }

    #[test]
    fn pid_reuse_with_new_executable_or_start_is_refused() {
        let identity = ProcessIdentity::bind(
            4242,
            Some("com.example.app"),
            "/Applications/Example.app/Contents/MacOS/Example",
            "launch-1",
        )
        .expect("binds");
        assert!(
            identity
                .revalidate(
                    4242,
                    Some("com.example.app"),
                    "/Applications/Other.app/Contents/MacOS/Other",
                    "launch-1",
                )
                .is_err()
        );
        assert!(
            identity
                .revalidate(
                    4242,
                    Some("com.example.app"),
                    "/Applications/Example.app/Contents/MacOS/Example",
                    "launch-2",
                )
                .is_err()
        );
        assert!(
            identity
                .revalidate(
                    4243,
                    Some("com.example.app"),
                    identity.exe.as_str(),
                    "launch-1"
                )
                .is_err()
        );
    }

    #[test]
    fn unscoped_binding_admits_ghostty_with_identical_lifetime_checks() {
        // Scoped entry points refuse Ghostty (pinned).
        assert!(
            ProcessIdentity::bind(4242, Some("com.mitchellh.ghostty"), "/x", "launch-1").is_err()
        );
        // Unscoped product entry admits Ghostty as an ordinary candidate.
        let identity =
            ProcessIdentity::bind_unscoped(4242, Some("com.mitchellh.ghostty"), "/x", "launch-1")
                .expect("ghostty binds unscoped");
        assert!(
            identity
                .revalidate_unscoped(4242, Some("com.mitchellh.ghostty"), "/x", "launch-1")
                .is_ok()
        );
        // ...and still refuses every drift exactly like the scoped path.
        assert!(
            identity
                .revalidate_unscoped(4243, Some("com.mitchellh.ghostty"), "/x", "launch-1")
                .is_err()
        );
        assert!(
            identity
                .revalidate_unscoped(4242, Some("com.example.other"), "/x", "launch-1")
                .is_err()
        );
        assert!(
            identity
                .revalidate_unscoped(4242, Some("com.mitchellh.ghostty"), "/y", "launch-1")
                .is_err()
        );
        assert!(
            identity
                .revalidate_unscoped(4242, Some("com.mitchellh.ghostty"), "/x", "launch-2")
                .is_err()
        );
        // Reserved PIDs and missing exe/start refuse on both paths.
        assert!(ProcessIdentity::bind_unscoped(1, Some("com.example.app"), "/x", "s").is_err());
        assert!(ProcessIdentity::bind_unscoped(4242, Some("com.example.app"), "", "s").is_err());
        assert!(ProcessIdentity::bind_unscoped(4242, Some("com.example.app"), "/x", "").is_err());
    }
}
