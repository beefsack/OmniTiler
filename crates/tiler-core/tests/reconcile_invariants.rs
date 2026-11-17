#![forbid(unsafe_code)]
//! Contract and boundary invariants.
//!
//! Retired C1 tests: 14 of the 16 original tests in this file drove the
//! deleted `tiler_core::reconcile::Reconciler` handshake/divergence machine
//! (`propose`, `acknowledge`, `verify`, `AdapterAck`, `PostObservation`,
//! `DivergenceKind`, `StateKind`) and are retired with C1.
//!
//! Exact count rationale: 16 -> 2 retained public tests:
//! - `session_ids_are_opaque_bounded_and_redacted` (retained public typed-ID
//!   bounds/parse/redaction assertions; `Reconciler`-specific dispatch/ack
//!   redaction tail retired).
//! - `portable_modules_prohibit_platform_imports` (retained platform-import
//!   prohibition over `contract.rs` and `world_layout/mod.rs`, replacing
//!   deleted `reconcile.rs`).

use tiler_core::contract::{MAX_CORRELATION_LEN, MAX_GENERATION_LEN, MAX_OWNER_LEN};
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};

#[test]
fn session_ids_are_opaque_bounded_and_redacted() {
    // Malformed session ids cannot construct typed ids (shared bounds).
    assert!(OwnerId::parse("").is_none());
    assert!(OwnerId::parse("bad id").is_none());
    assert!(OwnerId::parse(&"x".repeat(MAX_OWNER_LEN + 1)).is_none());
    assert!(GenerationId::parse("").is_none());
    assert!(GenerationId::parse("GEN-1").is_none());
    assert!(GenerationId::parse(&"g".repeat(MAX_GENERATION_LEN + 1)).is_none());
    assert!(CorrelationId::parse("").is_none());
    assert!(CorrelationId::parse("bad corr").is_none());
    assert!(CorrelationId::parse(&"c".repeat(MAX_CORRELATION_LEN + 1)).is_none());
    // Valid ids expose bytes only via as_str, never via Debug/Display.
    let owner = OwnerId::parse("owner-1").expect("valid");
    let generation = GenerationId::parse("gen-1").expect("valid");
    let correlation = CorrelationId::parse("corr-1").expect("valid");
    assert_eq!(owner.as_str(), "owner-1");
    assert_eq!(generation.as_str(), "gen-1");
    assert_eq!(correlation.as_str(), "corr-1");
    assert!(!format!("{owner:?}").contains("owner-1"));
    assert!(!format!("{generation:?}").contains("gen-1"));
    assert!(!format!("{correlation:?}").contains("corr-1"));
}

#[test]
fn portable_modules_prohibit_platform_imports() {
    // Compile-time inclusion: inspects the production sources without
    // touching host state, processes, or workdirs at test runtime.
    const CONTRACT: &str = include_str!("../src/contract.rs");
    const WORLD_LAYOUT: &str = include_str!("../src/world_layout/mod.rs");
    // Only `use`/`extern crate`/`mod` lines are import surface; comments
    // legitimately mention platform/process/IPC/geometry, so those words
    // alone must not count. Match import lines containing platform tokens.
    const BANNED: &[&str] = &[
        "zbus",
        "rustix",
        "dbus",
        "tokio",
        "async-std",
        "smol",
        "wayland",
        "x11",
        "kwin",
        "libc",
        "nix",
        "socket",
        "ipc",
        "process",
        "platform",
        "std::process",
        "std::net",
        "std::os",
        "std::fs",
        "std::env",
        "std::thread",
        "ffi",
    ];
    for (name, source) in [
        ("contract.rs", CONTRACT),
        ("world_layout/mod.rs", WORLD_LAYOUT),
    ] {
        for raw_line in source.lines() {
            let line = raw_line.trim().to_ascii_lowercase();
            if !(line.starts_with("use ")
                || line.starts_with("extern crate")
                || line.starts_with("mod "))
            {
                continue;
            }
            for token in BANNED {
                assert!(
                    !line.contains(token),
                    "{name} import line {raw_line:?} contains banned {token:?}"
                );
            }
        }
    }
    assert!(
        CONTRACT.contains("transport-independent") || CONTRACT.contains("transport-neutral"),
        "contract.rs should document its transport-independent boundary"
    );
}
