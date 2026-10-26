//! Scoped-tile effect-fence contract tests (offline, portable).
//!
//! Scripted gates drive the pure scoped fences through the public backend
//! surface: Silica-order frame transactions with per-leg truth, show-state
//! and membership refusals, PID-reuse identity admission, factory refusal
//! before any native touch, and whole-snapshot membership mapping. No live
//! AX, no taps, no Spaces, no desktop effects.

use std::collections::HashMap;

use tiler_core::geometry::Rect;
use tiler_macos::dev_selection::TileScope;
use tiler_macos::ffi::AxFrame;
use tiler_macos::focus_route::FocusRoute;
use tiler_macos::native_facts::NativeWindowFacts;
use tiler_macos::native_tile_backend::{
    AxSetter, LegOutcome, ScopedAxFactory, TileGates, WindowShowState, apply_scoped_frame,
    build_scoped_snapshot, fence_scoped_focus, session_safe_verdict, validate_identity_triple,
};
use tiler_macos::runtime_backend::AxBackendFactory;
use tiler_macos::runtime_types::AxCompletion;
use tiler_macos::scope::{AppScope, ProcessIdentity};
use tiler_macos::topology::PointsRect;

struct FakeGates {
    identity_ok: bool,
    bundle: Option<String>,
    cg_ok: bool,
    membership: Result<Vec<u64>, String>,
    current: Result<u64, String>,
    revision: u64,
    state: Result<WindowShowState, String>,
}

impl FakeGates {
    fn healthy(current: u64, revision: u64) -> Self {
        Self {
            identity_ok: true,
            bundle: Some("com.example.app".to_owned()),
            cg_ok: true,
            membership: Ok(vec![current]),
            current: Ok(current),
            revision,
            state: Ok(WindowShowState {
                fullscreen: false,
                minimized: false,
                hidden_app: false,
            }),
        }
    }
}

impl TileGates for FakeGates {
    fn revalidate_identity(&self) -> Result<(), String> {
        if self.identity_ok {
            Ok(())
        } else {
            Err("identity executable drift (possible PID reuse); refusing".to_owned())
        }
    }

    fn scope_bundle(&self) -> Option<String> {
        self.bundle.clone()
    }

    fn cg_owner(&self, _pid: u32, _wid: u32) -> Result<(), String> {
        if self.cg_ok {
            Ok(())
        } else {
            Err("wid not owned by pid; refusing".to_owned())
        }
    }

    fn membership(&self, _wid: u32) -> Result<Vec<u64>, String> {
        self.membership.clone()
    }

    fn current_user_space(&self) -> Result<u64, String> {
        self.current.clone()
    }

    fn topology_revision(&self) -> Result<u64, String> {
        Ok(self.revision)
    }

    fn show_state(&self, _wid: u32) -> Result<WindowShowState, String> {
        self.state.clone()
    }

    fn effect_alive(&self) -> Result<(), String> {
        Ok(())
    }

    fn session_safe(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Scripted setter mirroring the `ax.rs` `FrameTxOps` fake shape: an
/// owned current frame, one-shot leg failures, partial-mutation
/// emulation, and call counting — no AX, no threads, no live state.
/// Size legs apply width/height only and position legs origin only, so
/// per-leg readbacks report faithful partial truth.
struct FakeSetter {
    current: tiler_macos::ffi::AxFrame,
    position_fail_next: Option<String>,
    size_fail_on_call: Option<(usize, String, bool)>,
    size_calls: usize,
    get_fail_next: Option<String>,
    sets: Vec<&'static str>,
    set_calls: usize,
    get_calls: usize,
}

impl FakeSetter {
    fn at(frame: tiler_macos::ffi::AxFrame) -> Self {
        Self {
            current: frame,
            position_fail_next: None,
            size_fail_on_call: None,
            size_calls: 0,
            get_fail_next: None,
            sets: Vec::new(),
            set_calls: 0,
            get_calls: 0,
        }
    }

    fn ok() -> Self {
        Self::at(AxFrame {
            x: 10.0,
            y: 10.0,
            w: 800.0,
            h: 600.0,
        })
    }
}

impl AxSetter for FakeSetter {
    fn set_position(&mut self, _wid: u32, frame: &AxFrame) -> Result<(), String> {
        self.sets.push("position");
        self.set_calls += 1;
        if let Some(reason) = self.position_fail_next.take() {
            return Err(reason);
        }
        self.current.x = frame.x;
        self.current.y = frame.y;
        Ok(())
    }

    fn set_size(&mut self, _wid: u32, frame: &AxFrame) -> Result<(), String> {
        self.sets.push("size");
        self.set_calls += 1;
        self.size_calls += 1;
        // One-shot failure on exactly the Nth size call: peek first so
        // an earlier call never consumes the arming.
        let fires = self
            .size_fail_on_call
            .as_ref()
            .is_some_and(|(call, _, _)| *call == self.size_calls);
        if fires {
            let (_, reason, partial) = self.size_fail_on_call.take().expect("armed above");
            if partial {
                // Refused leg still moved one axis: the exact partial
                // hazard the per-leg readbacks must report.
                self.current.w = frame.w;
            }
            return Err(reason);
        }
        self.current.w = frame.w;
        self.current.h = frame.h;
        Ok(())
    }

    fn set_focus(&mut self, _wid: u32, _route: FocusRoute) -> Result<(), String> {
        self.sets.push("focus");
        Ok(())
    }

    fn readback(&mut self, _wid: u32) -> Result<Option<AxFrame>, String> {
        self.get_calls += 1;
        if let Some(reason) = self.get_fail_next.take() {
            return Err(reason);
        }
        Ok(Some(self.current))
    }
}

fn scope() -> TileScope {
    TileScope::Scoped(AppScope {
        allowed: vec!["com.example.app".to_owned()],
    })
}

fn identity() -> ProcessIdentity {
    ProcessIdentity::bind(
        4242,
        Some("com.example.app"),
        "/Applications/Example",
        "launch-1",
    )
    .expect("binds")
}

fn frame() -> AxFrame {
    AxFrame {
        x: 10.0,
        y: 10.0,
        w: 800.0,
        h: 600.0,
    }
}

fn attempt() -> AxFrame {
    AxFrame {
        x: 240.0,
        y: 240.0,
        w: 480.0,
        h: 320.0,
    }
}

#[test]
fn all_legs_apply_in_silica_order_with_per_leg_truth() {
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 9);
    let mut setter = FakeSetter::at(frame());
    let (completion, report) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &attempt(),
        9,
        3,
    );
    assert_eq!(setter.sets, vec!["size", "position", "size"]);
    assert_eq!(
        setter.get_calls, 3,
        "one bounded readback per attempted leg"
    );
    assert!(matches!(completion, AxCompletion::Applied { .. }));
    assert_eq!(report.size_first, LegOutcome::Applied);
    assert_eq!(report.position, LegOutcome::Applied);
    assert_eq!(report.size, LegOutcome::Applied);
    // Per-leg truth is faithful: after size-first only the size moved.
    let after_size_first = AxFrame {
        x: 10.0,
        y: 10.0,
        w: 480.0,
        h: 320.0,
    };
    assert_eq!(
        report.leg_readbacks,
        [Some(after_size_first), Some(attempt()), Some(attempt())]
    );
    assert_eq!(report.readback, Some(attempt()));
    assert!(!report.readback_unreadable);
}

#[test]
fn out_of_scope_never_writes() {
    let scope = scope();
    let identity = identity();
    let mut gates = FakeGates::healthy(11, 9);
    gates.bundle = Some("com.example.other".to_owned());
    let mut setter = FakeSetter::ok();
    let (completion, report) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &frame(),
        9,
        3,
    );
    assert!(matches!(completion, AxCompletion::Failed { .. }));
    assert!(setter.sets.is_empty(), "no setter may run out of scope");
    assert!(setter.get_calls == 0, "no readback without an attempt");
    assert!(matches!(report.position, LegOutcome::Refused(_)));
}

#[test]
fn ghostty_refused_at_effect_boundary_even_when_listed() {
    // Validation should already refuse this scope, but the boundary
    // refuses anyway (defense in depth).
    let scope = TileScope::Scoped(AppScope {
        allowed: vec!["com.mitchellh.ghostty".to_owned()],
    });
    let identity =
        ProcessIdentity::bind(4242, Some("com.example.app"), "/exe", "launch-1").expect("bind");
    let mut gates = FakeGates::healthy(11, 9);
    gates.bundle = Some("com.mitchellh.ghostty".to_owned());
    let mut setter = FakeSetter::ok();
    let (completion, _) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &frame(),
        9,
        0,
    );
    assert!(matches!(completion, AxCompletion::Failed { .. }));
    assert!(setter.sets.is_empty());
}

#[test]
fn other_space_membership_writes_nothing() {
    let scope = scope();
    let identity = identity();
    let mut gates = FakeGates::healthy(11, 9);
    gates.membership = Ok(vec![12]);
    let mut setter = FakeSetter::ok();
    let (completion, _) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &frame(),
        9,
        0,
    );
    assert!(matches!(completion, AxCompletion::Failed { .. }));
    assert!(setter.sets.is_empty(), "other-Space windows take no writes");
    // Sticky multi-Space likewise refuses.
    gates.membership = Ok(vec![11, 12]);
    let mut setter = FakeSetter::ok();
    let (completion, _) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &frame(),
        9,
        0,
    );
    assert!(matches!(completion, AxCompletion::Failed { .. }));
    assert!(setter.sets.is_empty());
}

#[test]
fn partial_size_refusal_still_attempts_rest_and_reports_truth() {
    // Final size leg refused after size-first+position applied: all
    // three legs still attempt in order, every attempted leg is read
    // back exactly once (no retry storm), and the completion reports
    // the actual partial truth.
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 9);
    let mut setter = FakeSetter::at(frame());
    setter.size_fail_on_call = Some((2, "size: kAXErrorCannotComplete".to_owned(), false));
    let (completion, report) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &attempt(),
        9,
        5,
    );
    assert_eq!(setter.sets, vec!["size", "position", "size"]);
    assert_eq!(setter.get_calls, 3);
    assert_eq!(report.size_first, LegOutcome::Applied);
    assert_eq!(report.position, LegOutcome::Applied);
    assert!(matches!(report.size, LegOutcome::Refused(_)));
    match completion {
        AxCompletion::PartialApplied {
            size_first_applied,
            position_applied,
            size_applied,
            ..
        } => {
            assert!(size_first_applied);
            assert!(position_applied);
            assert!(!size_applied);
        }
        other => panic!("expected partial completion, got {other:?}"),
    }
    assert!(report.readback.is_some());
}

#[test]
fn partial_first_size_mutation_reports_per_leg_truth() {
    // Refused first size leg partially moves one axis (the Silica
    // finickiness): the remaining legs still attempt, and the first
    // readback proves the partial state instead of the desired frame.
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 9);
    let mut setter = FakeSetter::at(frame());
    setter.size_fail_on_call = Some((1, "size: kAXErrorCannotComplete".to_owned(), true));
    let (completion, report) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &attempt(),
        9,
        5,
    );
    assert_eq!(setter.sets, vec!["size", "position", "size"]);
    assert!(matches!(report.size_first, LegOutcome::Refused(_)));
    assert_eq!(report.position, LegOutcome::Applied);
    assert_eq!(report.size, LegOutcome::Applied);
    // First readback shows width moved, everything else stale.
    assert_eq!(
        report.leg_readbacks[0],
        Some(AxFrame {
            x: 10.0,
            y: 10.0,
            w: 480.0,
            h: 600.0,
        })
    );
    assert!(matches!(completion, AxCompletion::PartialApplied { .. }));
}

#[test]
fn mid_pass_safety_refusal_skips_remaining_legs() {
    // Identity drifts between legs: the remaining legs skip with safety
    // evidence (never attempted blind), and the completion names the
    // fence rather than a setter cause.
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 9);
    let mut setter = FakeSetter::ok();
    // Flip identity unhealthy after the first fence by sharing state:
    // emulate with a gate whose revision drifts instead (same fence).
    struct Drifting {
        inner: FakeGates,
        calls: std::cell::Cell<u32>,
    }
    impl TileGates for Drifting {
        fn revalidate_identity(&self) -> Result<(), String> {
            self.inner.revalidate_identity()
        }
        fn scope_bundle(&self) -> Option<String> {
            self.inner.scope_bundle()
        }
        fn cg_owner(&self, pid: u32, wid: u32) -> Result<(), String> {
            self.inner.cg_owner(pid, wid)
        }
        fn membership(&self, wid: u32) -> Result<Vec<u64>, String> {
            self.inner.membership(wid)
        }
        fn current_user_space(&self) -> Result<u64, String> {
            self.inner.current_user_space()
        }
        fn topology_revision(&self) -> Result<u64, String> {
            let calls = self.calls.get() + 1;
            self.calls.set(calls);
            // Drift after the pre-fence (call 1): later fences refuse.
            if calls == 1 { Ok(9) } else { Ok(10) }
        }
        fn show_state(&self, wid: u32) -> Result<WindowShowState, String> {
            self.inner.show_state(wid)
        }
        fn effect_alive(&self) -> Result<(), String> {
            self.inner.effect_alive()
        }
        fn session_safe(&self) -> Result<(), String> {
            self.inner.session_safe()
        }
    }
    let gates = Drifting {
        inner: gates,
        calls: std::cell::Cell::new(0),
    };
    let (completion, report) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &attempt(),
        9,
        5,
    );
    assert_eq!(setter.sets, vec!["size"]);
    assert!(matches!(report.position, LegOutcome::SkippedSafety(_)));
    assert!(matches!(report.size, LegOutcome::SkippedSafety(_)));
    assert!(matches!(completion, AxCompletion::Failed { .. }));
}

#[test]
fn revision_drift_supersedes_without_writes() {
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 10);
    let mut setter = FakeSetter::ok();
    let (completion, _) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &frame(),
        9,
        0,
    );
    assert!(matches!(completion, AxCompletion::Failed { .. }));
    assert!(setter.sets.is_empty(), "stale revision writes nothing");
}

#[test]
fn minimized_hidden_and_fullscreen_take_no_writes() {
    let scope = scope();
    let identity = identity();
    let mut states = HashMap::new();
    states.insert(
        "minimized",
        WindowShowState {
            fullscreen: false,
            minimized: true,
            hidden_app: false,
        },
    );
    states.insert(
        "hidden",
        WindowShowState {
            fullscreen: false,
            minimized: false,
            hidden_app: true,
        },
    );
    states.insert(
        "fullscreen",
        WindowShowState {
            fullscreen: true,
            minimized: false,
            hidden_app: false,
        },
    );
    for (name, state) in states {
        let mut gates = FakeGates::healthy(11, 9);
        gates.state = Ok(state);
        let mut setter = FakeSetter::ok();
        let (completion, _) = apply_scoped_frame(
            &scope,
            &identity,
            &gates,
            &mut setter,
            4242,
            7,
            &frame(),
            9,
            0,
        );
        assert!(matches!(completion, AxCompletion::Failed { .. }), "{name}");
        assert!(setter.sets.is_empty(), "{name} takes no writes");
    }
}

#[test]
fn identity_triple_refuses_without_launch_date() {
    // Executable-only currency cannot distinguish PID reuse: a missing
    // launch date refuses even with a live executable path.
    assert!(
        validate_identity_triple(
            &scope(),
            4242,
            Some("com.example.app"),
            Some("/Applications/Example"),
            None,
        )
        .is_err()
    );
    assert!(
        validate_identity_triple(
            &scope(),
            4242,
            Some("com.example.app"),
            Some("/x"),
            Some("")
        )
        .is_err()
    );
    // Unknown/missing bundle refuses even with exe+launch date.
    assert!(validate_identity_triple(&scope(), 4242, None, Some("/x"), Some("launch-1")).is_err());
    // In-scope triple binds.
    let identity = validate_identity_triple(
        &scope(),
        4242,
        Some("com.example.app"),
        Some("/Applications/Example"),
        Some("launch-1"),
    )
    .expect("binds");
    assert_eq!(identity.pid, 4242);
    // Out-of-scope bundle refuses at admission (not just at effects).
    assert!(
        validate_identity_triple(
            &scope(),
            4242,
            Some("com.example.other"),
            Some("/Applications/Other"),
            Some("launch-1")
        )
        .is_err()
    );
    // Ghostty refuses at admission in any case.
    assert!(
        validate_identity_triple(
            &scope(),
            4242,
            Some("com.mitchellh.ghostty"),
            Some("/Applications/Ghostty"),
            Some("launch-1")
        )
        .is_err()
    );
}

#[test]
fn scoped_factory_refuses_dead_pid_without_native_touch() {
    // No such process: live identity reads come back empty and the
    // factory refuses before any AX element exists.
    let factory = ScopedAxFactory::new(AppScope {
        allowed: vec!["com.apple.TextEdit".to_owned()],
    })
    .expect("scope validates");
    assert!(factory.create(424242).is_err());
}

#[test]
fn build_scoped_snapshot_maps_complete_reads_with_membership() {
    use tiler_macos::native_facts::{NativeAppFacts, NativeReadOutcome, NativeWindowFacts};

    fn facts(wid: u32, x: f64) -> NativeWindowFacts {
        NativeWindowFacts {
            wid,
            role: Some(tiler_macos::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(tiler_macos::classifier::SUBROLE_STANDARD.to_owned()),
            fullscreen: Some(false),
            minimized: Some(false),
            modal: Some(false),
            size_settable: Some(true),
            min_size: None,
            max_size: None,
            frame: Some(AxFrame {
                x,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            }),
            cg_layer: Some(0),
            cg_onscreen: Some(true),
        }
    }

    let outcome = NativeReadOutcome {
        pid: 4242,
        requested: 2,
        observed: 2,
        failed: 0,
        first_cause: String::new(),
        windows: vec![facts(7, 0.0), facts(8, 200.0)],
        cg_complete: true,
        cg_owner_wids: vec![7, 8],
        cg_entries: Vec::new(),
    };
    let app = NativeAppFacts {
        pid: 4242,
        hidden: false,
        activation_policy: tiler_macos::classifier::ActivationPolicy::Regular,
        is_own_bundle: false,
    };
    let spaces_of = |wid: u32| match wid {
        7 => Ok(vec![11]),
        8 => Ok(vec![21]),
        _ => Ok(vec![]),
    };
    let snapshot = build_scoped_snapshot(4242, 9, &outcome, &app, &spaces_of, Some(7), true, false)
        .expect("complete read maps");
    assert_eq!(snapshot.revision, 9);
    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.meta.len(), 2);
    assert_eq!(snapshot.focused_wid, Some(7));
    assert!(snapshot.focus_cg_owned);
    assert!(snapshot.focus_evidence.is_none(), "worker stamps evidence");
    assert_eq!(snapshot.excluded, 0);
}

#[test]
fn build_scoped_snapshot_refuses_unhomed_and_incomplete_reads() {
    use tiler_macos::native_facts::{NativeAppFacts, NativeReadOutcome, NativeWindowFacts};

    fn facts(wid: u32) -> NativeWindowFacts {
        NativeWindowFacts {
            wid,
            role: Some(tiler_macos::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(tiler_macos::classifier::SUBROLE_STANDARD.to_owned()),
            fullscreen: Some(false),
            minimized: Some(false),
            modal: Some(false),
            size_settable: Some(true),
            min_size: None,
            max_size: None,
            frame: Some(AxFrame {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            }),
            cg_layer: Some(0),
            cg_onscreen: Some(true),
        }
    }

    let app = NativeAppFacts {
        pid: 4242,
        hidden: false,
        activation_policy: tiler_macos::classifier::ActivationPolicy::Regular,
        is_own_bundle: false,
    };
    // Every membership unreadable: whole read fails (nothing homable).
    let outcome = NativeReadOutcome {
        pid: 4242,
        requested: 1,
        observed: 1,
        failed: 0,
        first_cause: String::new(),
        windows: vec![facts(7)],
        cg_complete: true,
        cg_owner_wids: vec![7],
        cg_entries: Vec::new(),
    };
    assert!(
        build_scoped_snapshot(
            4242,
            9,
            &outcome,
            &app,
            &|_| Err("sls down".to_owned()),
            None,
            false,
            false
        )
        .is_err()
    );
    // Incomplete classification (unknown fullscreen): whole refusal.
    let mut gapped = facts(7);
    gapped.fullscreen = None;
    let outcome = NativeReadOutcome {
        pid: 4242,
        requested: 1,
        observed: 1,
        failed: 0,
        first_cause: String::new(),
        windows: vec![gapped],
        cg_complete: true,
        cg_owner_wids: vec![7],
        cg_entries: Vec::new(),
    };
    assert!(
        build_scoped_snapshot(
            4242,
            9,
            &outcome,
            &app,
            &|_| Ok(vec![11]),
            None,
            false,
            false
        )
        .is_err()
    );
}

#[test]
fn focus_fence_refuses_hidden_minimized_and_fullscreen() {
    // Conservative by code, not just by geometry: the focus fence carries
    // the same show-state recheck as the frame fence (a focus dispatch to a
    // window the compositor hides has no verifiable arrival, so the fence
    // withholds rather than dispatching blind).
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 9);
    assert!(
        fence_scoped_focus(&scope, &identity, &gates, 4242, 7, 9).is_ok(),
        "visible window dispatches"
    );
    for (name, state) in [
        (
            "minimized",
            WindowShowState {
                fullscreen: false,
                minimized: true,
                hidden_app: false,
            },
        ),
        (
            "hidden",
            WindowShowState {
                fullscreen: false,
                minimized: false,
                hidden_app: true,
            },
        ),
        (
            "fullscreen",
            WindowShowState {
                fullscreen: true,
                minimized: false,
                hidden_app: false,
            },
        ),
    ] {
        let mut gates = FakeGates::healthy(11, 9);
        gates.state = Ok(state);
        assert!(
            fence_scoped_focus(&scope, &identity, &gates, 4242, 7, 9).is_err(),
            "{name} takes no focus dispatch"
        );
    }
}

#[test]
fn bundled_helper_bundle_admits_with_explicit_scope() {
    // The signed bundled helper (stable helper bundle ID, LaunchServices
    // entry plus launch date) binds under an explicit helper-bundle scope:
    // bundle plus executable plus start token, all revalidated exactly.
    let helper_scope = TileScope::Scoped(AppScope {
        allowed: vec![tiler_macos::HELPER_BUNDLE_ID.to_owned()],
    });
    assert!(helper_scope.allows_effect(Some(tiler_macos::HELPER_BUNDLE_ID)));
    let identity = validate_identity_triple(
        &helper_scope,
        4242,
        Some(tiler_macos::HELPER_BUNDLE_ID),
        Some("/Applications/OmniTilerProbeHelper.app/Contents/MacOS/tiler-macos-probe-target"),
        Some("launch-1"),
    )
    .expect("bundled helper triple binds");
    assert_eq!(identity.pid, 4242);
    // Not admitted without an explicit listing: an everyday scope still
    // refuses the helper at admission.
    assert!(
        validate_identity_triple(
            &scope(),
            4242,
            Some(tiler_macos::HELPER_BUNDLE_ID),
            Some("/Applications/OmniTilerProbeHelper.app/Contents/MacOS/tiler-macos-probe-target"),
            Some("launch-1")
        )
        .is_err()
    );
}

#[test]
fn session_safety_refuses_secure_lock_and_unknown() {
    use tiler_macos::ffi::SessionLock;
    let ordinary = || Some((42u32, Some("com.apple.TextEdit".to_owned())));
    // Ordinary unlocked desktop proceeds (Secure Input unknown proceeds).
    assert!(session_safe_verdict(None, ordinary(), SessionLock::Unlocked).is_ok());
    // Secure Input active refuses even under an otherwise ordinary session.
    assert!(session_safe_verdict(Some(true), ordinary(), SessionLock::Unlocked).is_err());
    // Lock screen refuses; unknown frontmost refuses; locked and unknown
    // window-server sessions refuse.
    assert!(
        session_safe_verdict(
            Some(false),
            Some((7u32, Some("com.apple.loginwindow".to_owned()))),
            SessionLock::Unlocked,
        )
        .is_err()
    );
    assert!(session_safe_verdict(Some(false), None, SessionLock::Unlocked).is_err());
    assert!(session_safe_verdict(Some(false), ordinary(), SessionLock::Locked).is_err());
    assert!(session_safe_verdict(Some(false), ordinary(), SessionLock::Unknown).is_err());
}

#[test]
fn unscoped_effect_boundary_parity_with_scoped() {
    // Parity: Ghostty binds and passes the effect fence unscoped (ordinary
    // product candidate) while the scoped selection refuses it at admission;
    // own bundles, exclusions, and unknown bundles refuse in both modes.
    let unscoped = TileScope::Unscoped { excluded: vec![] };
    let ghostty = validate_identity_triple(
        &unscoped,
        4242,
        Some("com.mitchellh.ghostty"),
        Some("/Applications/Ghostty.app/Contents/MacOS/ghostty"),
        Some("launch-1"),
    )
    .expect("Ghostty binds unscoped");
    let mut gates = FakeGates::healthy(11, 9);
    gates.bundle = Some("com.mitchellh.ghostty".to_owned());
    assert!(
        fence_scoped_focus(&unscoped, &ghostty, &gates, 4242, 7, 9).is_ok(),
        "Ghostty passes the unscoped effect fence"
    );
    assert!(
        validate_identity_triple(
            &scope(),
            4242,
            Some("com.mitchellh.ghostty"),
            Some("/Applications/Ghostty.app/Contents/MacOS/ghostty"),
            Some("launch-1"),
        )
        .is_err(),
        "Ghostty refused at scoped admission"
    );
    // Own bundles refuse unscoped at admission (helper and probe alike).
    for own in [tiler_macos::BUNDLE_ID, tiler_macos::HELPER_BUNDLE_ID] {
        assert!(
            validate_identity_triple(&unscoped, 4242, Some(own), Some("/x"), Some("launch-1"))
                .is_err(),
            "{own}"
        );
    }
    // Exact exclusions refuse unscoped at admission; unknown refuses both.
    let excluded = TileScope::Unscoped {
        excluded: vec!["com.example.skip".to_owned()],
    };
    assert!(
        validate_identity_triple(
            &excluded,
            4242,
            Some("com.example.skip"),
            Some("/x"),
            Some("launch-1")
        )
        .is_err()
    );
    assert!(validate_identity_triple(&unscoped, 4242, None, Some("/x"), Some("launch-1")).is_err());
    // An unscoped factory constructs over valid exclusions and still
    // refuses a dead PID before any AX element exists (same shape as the
    // scoped factory test above).
    let factory = ScopedAxFactory::unscoped(vec!["com.example.skip".to_owned()])
        .expect("unscoped selection validates");
    assert!(factory.create(424242).is_err());
}

#[test]
fn cross_display_first_size_clamp_recovers_through_real_silica_legs() {
    // yabai window_manager.c 741-760 and AeroSpace MacApp.swift 411-419 both
    // land cross-display resizes as size-position-size against the
    // screen-area clamp: the first size leg clamps to the old display, the
    // position leg moves, and the second size leg recovers the target. The
    // fake below models exactly that first-leg clamp; every leg and
    // readback runs through production apply_scoped_frame.
    struct CrossDisplayClamp {
        current: AxFrame,
        clamped: AxFrame,
        size_calls: usize,
        sets: Vec<&'static str>,
    }
    impl AxSetter for CrossDisplayClamp {
        fn set_position(&mut self, _wid: u32, frame: &AxFrame) -> Result<(), String> {
            self.sets.push("position");
            self.current.x = frame.x;
            self.current.y = frame.y;
            Ok(())
        }
        fn set_size(&mut self, _wid: u32, frame: &AxFrame) -> Result<(), String> {
            self.sets.push("size");
            self.size_calls += 1;
            if self.size_calls == 1 {
                self.current.w = self.clamped.w;
                self.current.h = self.clamped.h;
            } else {
                self.current.w = frame.w;
                self.current.h = frame.h;
            }
            Ok(())
        }
        fn set_focus(&mut self, _wid: u32, _route: FocusRoute) -> Result<(), String> {
            Ok(())
        }
        fn readback(&mut self, _wid: u32) -> Result<Option<AxFrame>, String> {
            Ok(Some(self.current))
        }
    }
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 9);
    let clamped = AxFrame {
        x: 10.0,
        y: 10.0,
        w: 700.0,
        h: 500.0,
    };
    let target = AxFrame {
        x: 1500.0,
        y: 100.0,
        w: 1200.0,
        h: 800.0,
    };
    let mut setter = CrossDisplayClamp {
        current: frame(),
        clamped,
        size_calls: 0,
        sets: Vec::new(),
    };
    let (completion, report) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &target,
        9,
        3,
    );
    assert_eq!(setter.sets, vec!["size", "position", "size"]);
    assert!(
        matches!(completion, AxCompletion::Applied { .. }),
        "clamped first leg still lands: the second size leg recovers"
    );
    assert_eq!(report.size_first, LegOutcome::Applied);
    assert_eq!(report.position, LegOutcome::Applied);
    assert_eq!(report.size, LegOutcome::Applied);
    let moved = AxFrame {
        x: 1500.0,
        y: 100.0,
        w: 700.0,
        h: 500.0,
    };
    assert_eq!(
        report.leg_readbacks,
        [Some(clamped), Some(moved), Some(target)]
    );
    assert_eq!(report.readback, Some(target));
    assert!(!report.readback_unreadable);
    // Whole logical points reach the core through the real rounding seam.
    let points = PointsRect {
        x: target.x,
        y: target.y,
        w: target.w,
        h: target.h,
    };
    assert_eq!(
        points.to_rect(),
        Some(Rect {
            x: 1500,
            y: 100,
            w: 1200,
            h: 800,
        })
    );
}

#[test]
fn fractional_ax_readback_stays_raw_and_rounds_through_real_points_rect() {
    // The window server may read back fractional points immediately after
    // an integer set (async settle). Production keeps the raw fractional
    // readback in the report (never guessed) while planning rounds through
    // PointsRect::to_rect (nearest, ties away from zero). dpi_before/after
    // stay None on the macOS FrameAttempt path (pinned in world_builder).
    struct FractionalReadback {
        readback: AxFrame,
    }
    impl AxSetter for FractionalReadback {
        fn set_position(&mut self, _wid: u32, _frame: &AxFrame) -> Result<(), String> {
            Ok(())
        }
        fn set_size(&mut self, _wid: u32, _frame: &AxFrame) -> Result<(), String> {
            Ok(())
        }
        fn set_focus(&mut self, _wid: u32, _route: FocusRoute) -> Result<(), String> {
            Ok(())
        }
        fn readback(&mut self, _wid: u32) -> Result<Option<AxFrame>, String> {
            Ok(Some(self.readback))
        }
    }
    let scope = scope();
    let identity = identity();
    let gates = FakeGates::healthy(11, 9);
    let target = AxFrame {
        x: 240.0,
        y: 240.0,
        w: 480.0,
        h: 320.0,
    };
    let fractional = AxFrame {
        x: 240.5,
        y: 239.5,
        w: 480.4,
        h: 320.5,
    };
    let mut setter = FractionalReadback {
        readback: fractional,
    };
    let (completion, report) = apply_scoped_frame(
        &scope,
        &identity,
        &gates,
        &mut setter,
        4242,
        7,
        &target,
        9,
        3,
    );
    assert!(matches!(completion, AxCompletion::Applied { .. }));
    assert_eq!(
        report.readback,
        Some(fractional),
        "the report carries the raw fractional readback, never a guess"
    );
    let points = PointsRect {
        x: fractional.x,
        y: fractional.y,
        w: fractional.w,
        h: fractional.h,
    };
    assert_eq!(
        points.to_rect(),
        Some(Rect {
            x: 241,
            y: 240,
            w: 480,
            h: 321,
        }),
        "planning rounds the fractional readback through the real seam"
    );
    // Ties round away from zero through the same production seam.
    assert_eq!(
        PointsRect {
            x: 0.5,
            y: -0.5,
            w: 10.0,
            h: 10.0,
        }
        .to_rect()
        .map(|r| (r.x, r.y)),
        Some((1, -1))
    );
    // Physical fixture minima ceil at scale through production size_hints;
    // maxima stay absent (no surveyed AX max source: never invented).
    let scale = 2.0;
    let logical_min_w = 1281.0 / scale;
    assert_eq!(logical_min_w, 640.5);
    let mut facts = NativeWindowFacts {
        wid: 7,
        role: Some("AXWindow".to_owned()),
        subrole: Some("AXStandardWindow".to_owned()),
        fullscreen: Some(false),
        minimized: Some(false),
        modal: Some(false),
        size_settable: Some(true),
        min_size: Some((logical_min_w, 480.25)),
        max_size: None,
        frame: Some(target),
        cg_layer: Some(0),
        cg_onscreen: Some(true),
    };
    let hints = facts.size_hints();
    assert_eq!((hints.min_w, hints.min_h), (Some(641), Some(481)));
    assert!(
        hints.max_w.is_none() && hints.max_h.is_none(),
        "no AX max is probed, so none is ever filled"
    );
    // Non-positive and non-finite minima stay absent, never a claim.
    for bad in [0.0, -3.0, f64::NAN, f64::INFINITY] {
        facts.min_size = Some((bad, 480.0));
        assert!(
            facts.size_hints().min_w.is_none(),
            "bad minimum {bad} must not fill a hint"
        );
    }
}
