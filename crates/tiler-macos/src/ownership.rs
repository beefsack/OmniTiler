#![forbid(unsafe_code)]

//! Owned-probe identity and ownership gates.
//!
//! Never act on Ghostty, the controlling terminal, or arbitrary windows.
//! Every mutating AX/input/Space path must validate [`OwnedIdentity`] first,
//! and caller claims alone are not proof: [`verify_live_process`] rechecks
//! the PID against the live running-application table (bundle) and the live
//! executable path before any write or kill. Reused PIDs never authorize
//! writes to another window.

use serde::{Deserialize, Serialize};

/// Controlling-terminal / known-host-terminal bundle IDs that are always
/// protected, even if a PID coincidence is claimed.
const PROTECTED_BUNDLE_SUBSTRINGS: &[&str] = &[
    "ghostty",
    "terminal",
    "iterm",
    "wezterm",
    "kitty",
    "alacritty",
    "vscode",
];

/// True when `bundle_id` is one of the two known owned probe identities:
/// the probe bundle itself (direct spawns, which carry no LaunchServices
/// entry and bind via executable path) or the dedicated signed helper
/// bundle (bundled-helper spawns, which carry a LaunchServices entry plus
/// launch date). Anything else — including empty/unknown — is never owned.
#[must_use]
pub fn is_owned_bundle(bundle_id: &str) -> bool {
    bundle_id == crate::BUNDLE_ID || bundle_id == crate::HELPER_BUNDLE_ID
}

/// Identity of the owned disposable probe target process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedIdentity {
    /// PID of the spawned `probe_target` child.
    pub pid: u32,
    /// Must equal [`crate::BUNDLE_ID`] or [`crate::HELPER_BUNDLE_ID`]
    /// (see [`is_owned_bundle`]).
    pub bundle_id: String,
    /// Opaque nonce issued at spawn; ≥16 hex chars.
    pub nonce: String,
}

impl OwnedIdentity {
    pub fn new(pid: u32, bundle_id: &str, nonce: &str) -> Result<Self, String> {
        if pid <= 1 {
            return Err(format!("refusing identity for reserved pid {pid}"));
        }
        if !is_owned_bundle(bundle_id) {
            return Err(format!(
                "refusing identity: bundle '{bundle_id}' != owned '{}' or '{}'",
                crate::BUNDLE_ID,
                crate::HELPER_BUNDLE_ID
            ));
        }
        validate_nonce(nonce)?;
        Ok(Self {
            pid,
            bundle_id: bundle_id.to_owned(),
            nonce: nonce.to_owned(),
        })
    }

    /// Validate that `target` is this exact owned process. `target_bundle`
    /// is the live-observed bundle of `target_pid`; any protected bundle,
    /// any bundle outside the two known owned IDs, or PID mismatch fails
    /// closed.
    pub fn check_action(&self, target_pid: u32, target_bundle: &str) -> Result<(), String> {
        if is_protected_bundle(target_bundle) {
            return Err(format!(
                "refusing action on protected bundle '{target_bundle}'"
            ));
        }
        if !is_owned_bundle(target_bundle) {
            return Err(format!(
                "refusing action: target bundle '{target_bundle}' is not a known owned bundle"
            ));
        }
        if target_bundle != self.bundle_id {
            return Err(format!(
                "refusing action: target bundle '{target_bundle}' != owned '{}'",
                self.bundle_id
            ));
        }
        if target_pid != self.pid {
            return Err(format!(
                "refusing action: target pid {target_pid} != owned pid {}",
                self.pid
            ));
        }
        Ok(())
    }
}

pub fn validate_nonce(nonce: &str) -> Result<(), String> {
    if nonce.len() < 16 {
        return Err("nonce too short (need >=16 chars)".to_owned());
    }
    if !nonce.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("nonce must be hex".to_owned());
    }
    Ok(())
}

pub fn is_protected_bundle(bundle_id: &str) -> bool {
    let lower = bundle_id.to_ascii_lowercase();
    PROTECTED_BUNDLE_SUBSTRINGS
        .iter()
        .any(|pat| lower.contains(pat))
}

/// Live verification of an owned process: the PID must currently resolve to
/// an owned bundle ID (the probe bundle, or the dedicated signed helper
/// bundle for the regular-policy focus fixture) in the running-application
/// table, or — for a directly spawned helper with no LaunchServices bundle
/// entry — to the expected helper executable path. Child-owned contract: exact
/// `Child` PID + executable-path binding + per-run nonce (checked by the spawn
/// record) + CURRENT live window-nonce title over a fresh CGWindowList
/// snapshot at use. Caller-supplied PIDs/nonces that fail any live check are
/// refused; PID reuse cannot authorize another process.
pub fn verify_live_process(pid: u32, expected_exe: Option<&str>) -> Result<String, String> {
    if pid <= 1 {
        return Err(format!("refusing reserved pid {pid}"));
    }
    let pid_i32: i32 = pid
        .try_into()
        .map_err(|_| format!("pid {pid} out of range"))?;
    match crate::ffi::bundle_id_for_pid(pid_i32).filter(|b| !b.is_empty()) {
        Some(bundle) => {
            if is_protected_bundle(&bundle) {
                return Err(format!("refusing action on protected bundle '{bundle}'"));
            }
            if bundle != crate::BUNDLE_ID && bundle != crate::HELPER_BUNDLE_ID {
                return Err(format!(
                    "refusing action: live bundle '{bundle}' != owned '{}' or '{}'",
                    crate::BUNDLE_ID,
                    crate::HELPER_BUNDLE_ID
                ));
            }
            if let Some(expected) = expected_exe {
                let live = crate::ffi::process_executable_path(pid_i32)
                    .ok_or_else(|| format!("pid {pid}: executable path unreadable"))?;
                if live != expected {
                    return Err(
                        "refusing action: live executable differs (possible PID reuse)".to_owned(),
                    );
                }
            }
            Ok(bundle)
        }
        // No LaunchServices bundle entry (directly spawned helper): bind via
        // the expected executable path plus liveness. No process-identity
        // machinery here; the spawn record's nonce plus the CURRENT live
        // window-nonce title check at use complete the contract.
        None => {
            let expected = expected_exe.ok_or_else(|| {
                format!("pid {pid} has no live bundle; refusing without executable binding")
            })?;
            let live = crate::ffi::process_executable_path(pid_i32)
                .ok_or_else(|| format!("pid {pid}: executable path unreadable"))?;
            if live != expected {
                return Err(
                    "refusing action: live executable differs (possible PID reuse)".to_owned(),
                );
            }
            Ok(crate::BUNDLE_ID.to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned() -> OwnedIdentity {
        OwnedIdentity::new(4242, crate::BUNDLE_ID, "0123456789abcdef").unwrap()
    }

    #[test]
    fn rejects_wrong_bundle() {
        assert!(OwnedIdentity::new(4242, "com.example.other", "0123456789abcdef").is_err());
        assert!(OwnedIdentity::new(4242, "", "0123456789abcdef").is_err());
    }

    #[test]
    fn accepts_both_known_owned_bundles() {
        // Direct spawns bind the probe bundle; bundled-helper spawns bind
        // the dedicated signed helper bundle. Both are owned; anything
        // else (including empty/unknown) refuses.
        assert!(OwnedIdentity::new(4242, crate::BUNDLE_ID, "0123456789abcdef").is_ok());
        assert!(OwnedIdentity::new(4242, crate::HELPER_BUNDLE_ID, "0123456789abcdef").is_ok());
        assert!(!is_owned_bundle("com.example.other"));
        assert!(!is_owned_bundle(""));
        assert!(!is_owned_bundle("com.mitchellh.ghostty"));
        let helper = OwnedIdentity::new(4242, crate::HELPER_BUNDLE_ID, "0123456789abcdef").unwrap();
        assert!(helper.check_action(4242, crate::HELPER_BUNDLE_ID).is_ok());
        // Cross-bundle use refuses: a helper identity never authorizes the
        // probe bundle and vice versa.
        assert!(helper.check_action(4242, crate::BUNDLE_ID).is_err());
        assert!(owned().check_action(4242, crate::HELPER_BUNDLE_ID).is_err());
        // Unknown live bundles refuse even against a valid identity.
        assert!(owned().check_action(4242, "com.example.other").is_err());
    }

    #[test]
    fn rejects_reserved_pid() {
        assert!(OwnedIdentity::new(1, crate::BUNDLE_ID, "0123456789abcdef").is_err());
    }

    #[test]
    fn rejects_bad_nonce() {
        assert!(OwnedIdentity::new(42, crate::BUNDLE_ID, "short").is_err());
        assert!(OwnedIdentity::new(42, crate::BUNDLE_ID, "zzzzzzzzzzzzzzzz").is_err());
    }

    #[test]
    fn refuses_ghostty_and_terminals() {
        assert!(is_protected_bundle("com.mitchellh.ghostty"));
        assert!(is_protected_bundle("com.apple.Terminal"));
        assert!(!is_protected_bundle(crate::BUNDLE_ID));
        let id = owned();
        assert!(id.check_action(4242, "com.mitchellh.ghostty").is_err());
    }

    #[test]
    fn pid_mismatch_fails_closed() {
        let id = owned();
        assert!(id.check_action(4242, crate::BUNDLE_ID).is_ok());
        assert!(id.check_action(4243, crate::BUNDLE_ID).is_err());
    }

    #[test]
    fn live_verify_rejects_dead_pid() {
        // No such process: live lookup must fail, never fall back to claims.
        assert!(verify_live_process(4242, None).is_err());
    }

    #[test]
    fn live_verify_rejects_dead_pid_even_with_exe_binding() {
        // An exited PID fails closed even when the caller supplies the
        // expected executable: no live exe means no bundle-less fallback.
        assert!(verify_live_process(4242, Some("/nonexistent/helper")).is_err());
    }
}
