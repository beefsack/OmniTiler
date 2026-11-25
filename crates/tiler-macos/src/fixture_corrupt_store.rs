#![forbid(unsafe_code)]

//! `corrupt-intent-store` diagnostic support for `fixture-leg`.
//!
//! The leg pre-seeds its isolated product directory's intent store with
//! malformed JSON before the product launches, then requires one bounded
//! `intent-store:` product diagnostic from the run's stderr after the chords
//! and before teardown. This module owns only the pre-seed and the bounded
//! capture; the pre-seeded leaf is left in place for the post-shutdown
//! `live_leg` teardown, which owns it and removes it after the product stops.
//! Never delete the leaf mid-run.

use std::path::Path;

/// Pre-seed the isolated product intent store with malformed JSON before the
/// product launches. The leaf stays in place for post-shutdown `live_leg`
/// teardown to remove.
pub(crate) fn pre_seed(product_dir: &Path) -> Result<(), (i32, String)> {
    use std::os::unix::fs::PermissionsExt;
    let (uid, asid) = crate::ffi::audit_self()
        .map_err(|e| (1, format!("fixture-leg: audit identity unreadable ({e})")))?;
    let store_dir = product_dir.join("intent-store");
    std::fs::create_dir_all(&store_dir)
        .map_err(|e| (1, format!("fixture-leg: corrupt store dir refused ({e})")))?;
    std::fs::set_permissions(&store_dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| {
        (
            1,
            format!("fixture-leg: corrupt store dir mode refused ({e})"),
        )
    })?;
    let path = store_dir.join(format!("intent-v1-{uid}-{asid}.json"));
    std::fs::write(&path, b"{}")
        .map_err(|e| (1, format!("fixture-leg: corrupt store write refused ({e})")))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| (1, format!("fixture-leg: corrupt store mode refused ({e})")))?;
    Ok(())
}

/// Capture the product's bounded `intent-store:` diagnostic from the run's
/// stderr. The pre-seeded leaf is left in place here: post-shutdown `live_leg`
/// teardown owns its removal. Removing it while the product is still running
/// bypassed symlink safety and stranded it on every mid-run error.
pub(crate) fn capture(product_dir: &Path) -> Result<serde_json::Value, (i32, String)> {
    let stderr = std::fs::read_to_string(product_dir.join("stderr.log")).unwrap_or_default();
    let diagnostic = stderr
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("intent-store:"))
        .map(str::to_owned);
    let diagnostic = diagnostic.ok_or((
        1,
        "fixture-leg: corrupt store produced no product diagnostic".to_owned(),
    ))?;
    let body = serde_json::json!({
        "outcome": "degraded",
        "diagnostic": diagnostic,
        "note": "pre-seeded malformed intent JSON; the product launched, tiled, and shut down cleanly with the store failed closed"
    });
    Ok(body)
}
