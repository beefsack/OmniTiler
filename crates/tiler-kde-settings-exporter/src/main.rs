//! Development-only KDE settings exporter.
//!
//! Regenerates the checked-in KDE descriptors from the shared catalog:
//! `kwin/native-effect/activeborderconfig.kcfg`,
//! `kwin/contents/config/main.xml` and
//! `kwin/native-effect/generated/kde_settings_descriptors.h`.
//!
//! Usage: `cargo run -p tiler-kde-settings-exporter [--check]`.
//! Without flags the artifacts are rewritten in place; `--check` only
//! reports stale or missing artifacts and exits nonzero. The settings-only
//! CMake build consumes the checked-in files and never runs this tool.

mod emit;
mod kde_adapter;

use std::path::{Path, PathBuf};

use emit::{ARTIFACTS, contents_match};
use kde_adapter::{KDE_EXPOSED_FIELDS, KDE_SCHEMA_ONLY_FIELD, KDE_UNSUPPORTED_FIELDS};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Check (or refresh) every artifact under `root`. Returns the relative
/// paths that were stale or missing. Pure logic behind the CLI exit code,
/// so tests exercise the real drift gate without touching the repo.
fn run(root: &Path, check: bool) -> Vec<String> {
    let mut stale = Vec::new();
    for (relative, generate) in ARTIFACTS {
        let path = root.join(relative);
        let emitted = generate();
        match std::fs::read_to_string(&path) {
            Ok(current) if contents_match(&current, &emitted) => {}
            _ => {
                stale.push(relative.to_string());
            }
        }
        if !check && let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("artifact dir writable");
            std::fs::write(&path, emitted).expect("artifact writable");
        }
    }
    stale
}

fn main() {
    let check = std::env::args().any(|arg| arg == "--check");
    println!(
        "kde coverage: {} exposed, {} unsupported, 1 schema-only ({})",
        KDE_EXPOSED_FIELDS.len(),
        KDE_UNSUPPORTED_FIELDS.len(),
        KDE_SCHEMA_ONLY_FIELD
    );
    let stale = run(&repo_root(), check);
    for (relative, _) in ARTIFACTS {
        if stale.iter().any(|stale| stale == relative) {
            println!("stale: {relative}");
        } else {
            println!("ok: {relative}");
        }
    }
    if check && !stale.is_empty() {
        eprintln!("stale KDE descriptors: {}", stale.join(", "));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);

    fn scratch_root() -> PathBuf {
        let id = SCRATCH_SEQ.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "tiler-kde-exporter-check-{}-{}",
            std::process::id(),
            id
        ))
    }

    #[test]
    fn check_fails_on_missing_artifacts_and_passes_after_emit() {
        let root = scratch_root();
        // Missing artifacts are drift: non-empty stale list (exit 1).
        let stale = run(&root, true);
        assert_eq!(stale.len(), ARTIFACTS.len());
        // Emit mode writes every artifact, then the gate passes.
        let stale = run(&root, false);
        assert_eq!(stale.len(), ARTIFACTS.len());
        assert!(run(&root, true).is_empty());
        std::fs::remove_dir_all(&root).expect("scratch cleanup");
    }

    #[test]
    fn check_fails_on_single_byte_drift() {
        let root = scratch_root();
        assert!(!run(&root, false).is_empty());
        assert!(run(&root, true).is_empty());
        // Tamper with one checked-in byte; the real gate must catch it.
        let (relative, _) = ARTIFACTS[0];
        let path = root.join(relative);
        let mut contents = std::fs::read_to_string(&path).expect("artifact written");
        contents.replace_range(10..11, "X");
        std::fs::write(&path, contents).expect("tamper writable");
        assert_eq!(run(&root, true), vec![relative.to_string()]);
        std::fs::remove_dir_all(&root).expect("scratch cleanup");
    }
}
