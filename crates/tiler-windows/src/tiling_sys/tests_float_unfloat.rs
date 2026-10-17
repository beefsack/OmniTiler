#![forbid(unsafe_code)]
use super::UnfloatClearEffects;
use super::rmax03_adapter_tests::test_state;
use tiler_core::geometry::Rect;

fn float_key(hwnd: u64) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid: 7,
        creation: "creation-float".to_owned(),
    }
}

fn float_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn float_loc(output: &str, workspace: &str) -> crate::workspace::MemberLoc {
    crate::workspace::MemberLoc {
        output: output.to_owned(),
        workspace: workspace.to_owned(),
        hidden: false,
    }
}

fn maximized_retained(key: &crate::workspace::WindowKey, token: &str) -> super::RetainedRow {
    super::RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: Some(Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }),
        maximized: true,
        fullscreen: false,
        facts: None,
    }
}

/// Stage membership tables for one fake-HWND member (deterministic,
/// never a real window): every native query fails closed with no effect.
fn stage_member(
    state: &mut super::TileLoop,
    output: &str,
    active: &str,
    key: &crate::workspace::WindowKey,
    token: &str,
) {
    assert!(state.workspaces.assign(key.clone(), output, active, false));
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        },
    );
    state
        .member_tags
        .insert(key.clone(), "float-tag".to_owned());
}

/// Scripted B9 native effects (test-only): deterministic answers for
/// every Win32 read/write on the clear route, plus a call log pinning
/// production ordering. Message-only owned windows cannot carry this
/// path (never foreground, never enumerated, machine-dependent suspend
/// reads), so scripts — not the `AdoptWindow` harness — are the
/// hermetic route here.
struct ScriptedClear {
    revalidate_err: Option<&'static str>,
    revalidate_fail_second: bool,
    revalidate_captionless: bool,
    foreground_pre: u64,
    foreground_post: u64,
    suspend_block: bool,
    elevated: bool,
    meta_pre: Option<super::FullscreenMeta>,
    meta_cur_set: bool,
    meta_cur_val: Option<super::FullscreenMeta>,
    meta_post: Option<super::FullscreenMeta>,
    retire_style: Option<u32>,
    retire_fullscreen: Option<bool>,
    retire_probe2_set: bool,
    retire_style2: Option<u32>,
    retire_fullscreen2: Option<bool>,
    retire_clear_ok: bool,
    retire_clear_called: bool,
    verify_returned: bool,
    restore: &'static str,
    pid_current: bool,
    observe: Option<super::FreshFloatObservation>,
    relocate_post_clear: Option<(crate::workspace::WindowKey, String, String)>,
    fg_calls: u32,
    meta_calls: u32,
    revalidate_calls: u32,
    probe_calls: u32,
    calls: Vec<&'static str>,
}

impl ScriptedClear {
    fn passing(fg: u64, fresh: super::FreshFloatObservation) -> Self {
        ScriptedClear {
            revalidate_err: None,
            revalidate_fail_second: false,
            revalidate_captionless: false,
            foreground_pre: fg,
            foreground_post: fg,
            suspend_block: false,
            elevated: false,
            meta_pre: None,
            meta_cur_set: false,
            meta_cur_val: None,
            meta_post: None,
            retire_style: None,
            retire_fullscreen: None,
            retire_probe2_set: false,
            retire_style2: None,
            retire_fullscreen2: None,
            retire_clear_ok: true,
            retire_clear_called: false,
            verify_returned: false,
            restore: "restored",
            pid_current: true,
            observe: Some(fresh),
            relocate_post_clear: None,
            fg_calls: 0,
            meta_calls: 0,
            revalidate_calls: 0,
            probe_calls: 0,
            calls: Vec::new(),
        }
    }
}

impl UnfloatClearEffects for ScriptedClear {
    fn revalidate(
        &mut self,
        _state: &mut super::TileLoop,
        _member: &crate::workspace::WindowKey,
        expected: &super::ObservedWindow,
        _me: &crate::model::ProcessIdentity,
        _fulls: &[Rect],
    ) -> std::result::Result<super::WriteTarget, &'static str> {
        self.calls.push("revalidate");
        self.revalidate_calls += 1;
        if let Some(reason) = self.revalidate_err {
            return Err(reason);
        }
        if self.revalidate_fail_second && self.revalidate_calls >= 2 {
            return Err("identity-changed");
        }
        let mut window = expected.clone();
        window.facts.captionless_fullscreen = self.revalidate_captionless;
        let held =
            crate::native::HeldProcess::open(std::process::id()).expect("stub holds own process");
        Ok(super::WriteTarget { window, held })
    }

    fn foreground(&mut self) -> u64 {
        self.calls.push("foreground");
        self.fg_calls += 1;
        if self.fg_calls == 1 {
            self.foreground_pre
        } else {
            self.foreground_post
        }
    }

    fn suspend_block(
        &mut self,
        _state: &mut super::TileLoop,
        _me: &crate::model::ProcessIdentity,
        _fulls: &[Rect],
    ) -> bool {
        self.calls.push("suspend");
        self.suspend_block
    }

    fn elevated(&mut self, _me: &crate::model::ProcessIdentity) -> bool {
        self.calls.push("elevated");
        self.elevated
    }

    fn fullscreen_meta(&mut self, _hwnd: u64) -> Option<super::FullscreenMeta> {
        self.calls.push("meta");
        self.meta_calls += 1;
        // Read role follows the production order, not the call count, so
        // the B9 route and the background sweep share one double: the
        // post-clear post fence always runs after `observe`, a pre-read
        // runs before any retire probe, the equality re-read runs between
        // the pin and the removal, and the removal verify runs once right
        // after the clear.
        if self.calls.contains(&"observe") {
            self.meta_post
        } else if self.probe_calls == 0 {
            self.meta_pre
        } else if !self.retire_clear_called {
            if self.meta_cur_set {
                self.meta_cur_val
            } else {
                self.meta_pre
            }
        } else if !self.verify_returned {
            self.verify_returned = true;
            if self.retire_clear_ok {
                None
            } else {
                self.meta_pre
            }
        } else {
            self.meta_post
        }
    }

    fn retire_probe(&mut self, _hwnd: u64, _fulls: &[Rect]) -> (Option<u32>, Option<bool>) {
        self.calls.push("retire-probe");
        self.probe_calls += 1;
        if self.probe_calls >= 2 && self.retire_probe2_set {
            (self.retire_style2, self.retire_fullscreen2)
        } else {
            (self.retire_style, self.retire_fullscreen)
        }
    }

    fn retire_clear(&mut self, _hwnd: u64) -> bool {
        self.calls.push("retire-clear");
        self.retire_clear_called = true;
        self.retire_clear_ok
    }

    fn restore(&mut self, _hwnd: u64) -> &'static str {
        self.calls.push("restore");
        self.restore
    }

    fn pid_current(&mut self, _hwnd: u64, _pid: u32) -> bool {
        self.calls.push("pid");
        self.pid_current
    }

    fn observe(
        &mut self,
        state: &mut super::TileLoop,
        _me: &crate::model::ProcessIdentity,
        _fulls: &[Rect],
    ) -> Option<super::FreshFloatObservation> {
        self.calls.push("observe");
        if let Some((key, output, ws)) = self.relocate_post_clear.clone() {
            assert!(
                state.workspaces.assign(key, &output, &ws, false),
                "race target workspace exists"
            );
        }
        self.observe.clone()
    }
}

#[test]
fn refused_clear_retains_float_with_no_commit_or_attempt() {
    // B9 fail-closed leg through real adapter code: a retained maximized
    // float on a fake HWND fails identity revalidation before any native
    // write, so the toggle ends with float membership, preimages, and
    // Engine state untouched, no clear attempt recorded, and no tile
    // override installed. A later press retries.
    let mut state = test_state();
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    let key = float_key(101);
    stage_member(&mut state, &output, &active, &key, "w8");
    state.floated.insert(key.clone());
    let retained = vec![maximized_retained(&key, "w8")];
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut super::NativeUnfloatClear,
        &float_me(),
        &[],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("fake HWND must fail closed");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("pid-changed")),
        "revalidation races end the toggle before any write"
    );
    assert!(
        state.member_tokens.contains_key(&key),
        "refused clear retains membership"
    );
    assert!(
        state.floated.contains(&key),
        "refused clear retains float intent"
    );
    assert!(
        state.member_identity.contains_key(&key) && state.member_tags.contains_key(&key),
        "refused clear retains preimages for a later press"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "B9 records no one-shot attempt: every press tries once"
    );
    assert!(
        !state.fixed_tile_overrides.contains(&key) && !state.fixed_origin_overrides.contains(&key),
        "no admission means no intent marker override"
    );
}

#[test]
fn identity_skew_clears_nothing_and_keeps_plain_state() {
    // The clear must pin the chord-time member key exactly: an observed
    // token bound to another HWND races closed with no native write.
    let mut state = test_state();
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    let key = float_key(101);
    stage_member(&mut state, &output, &active, &key, "w8");
    state.floated.insert(key.clone());
    let observed = vec![super::ObservedWindow {
        hwnd: 102,
        token: "w8".to_owned(),
        outer: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        visible: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: 102,
            pid: 9,
            process_creation: "creation-other".to_owned(),
            exe_path: "test.exe".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }];
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut super::NativeUnfloatClear,
        &float_me(),
        &[],
        &observed,
        &[],
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("skewed observation must race closed");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("identity-raced"))
    );
    assert!(state.member_tokens.contains_key(&key));
    assert!(state.floated.contains(&key));
    assert!(state.maximize_admission_attempted.is_empty());
}

#[test]
fn unknown_subject_keeps_plain_refusal_with_no_clear() {
    // No observed or retained row for the token: the plain snapshot
    // refusal stands and no clear is attempted.
    let mut state = test_state();
    let loc = float_loc("mon-1", "ws-1");
    let key = float_key(101);
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut super::NativeUnfloatClear,
        &float_me(),
        &[],
        &[],
        &[],
        &key,
        "w-missing",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("unknown subject attempts no clear");
    assert_eq!(err, ("float-refused-maximize", None));
    assert!(state.maximize_admission_attempted.is_empty());
}

#[test]
fn fullscreen_and_sticky_lanes_stay_plain_refusals() {
    // Hermetic adapter seam: a fullscreen float refuses in both
    // vocabularies regardless of the (fake, reads-clear) zoom flag, so
    // production keeps those lanes on the plain refusal branch; the pure
    // lane predicate pins the full matrix beside it.
    let key = float_key(101);
    let fullscreen_observed = vec![super::ObservedWindow {
        hwnd: 101,
        token: "w8".to_owned(),
        outer: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        },
        visible: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        },
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: 101,
            pid: 7,
            process_creation: "creation-float".to_owned(),
            exe_path: "test.exe".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: true,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: true,
            no_activate: false,
            dialog: false,
        },
    }];
    assert_eq!(
        super::chord_overlay_refusal(false, &fullscreen_observed, &[], &key),
        Some("float-refused-fullscreen"),
        "fullscreen floats never enter the B9 lane"
    );
    assert_eq!(
        super::chord_overlay_refusal(true, &fullscreen_observed, &[], &key),
        Some("sticky-refused-fullscreen"),
        "sticky vocabulary unchanged"
    );
    let fullscreen_retained = vec![super::RetainedRow {
        key: key.clone(),
        token: "w8".to_owned(),
        rect: None,
        maximized: false,
        fullscreen: true,
        facts: None,
    }];
    assert_eq!(
        super::chord_overlay_refusal(false, &[], &fullscreen_retained, &key),
        Some("float-refused-fullscreen")
    );
    assert_eq!(
        super::chord_overlay_refusal(true, &[], &fullscreen_retained, &key),
        Some("sticky-refused-fullscreen")
    );
    // Pure lane matrix beside the seam: only the ordinary maximized
    // float proceeds to the clear route.
    use crate::tiling::unfloat_maximize_clear_eligible as lane;
    assert!(
        lane(false, true, true, false),
        "ordinary maximized float clears"
    );
    assert!(!lane(true, true, true, false), "fullscreen never clears");
    assert!(
        !lane(true, false, true, false),
        "fullscreen wins over maximize"
    );
    assert!(!lane(false, true, true, true), "sticky keeps its refusal");
    assert!(
        !lane(false, true, false, false),
        "tiled targets never clear"
    );
    assert!(
        !lane(false, false, true, false),
        "normal floats admit directly"
    );
    assert!(
        crate::tiling::float_toggle_refusal(false, true) == Some("float-refused-maximize")
            && lane(false, true, true, false),
        "lane matches the snapshot vocabulary it replaces"
    );
}

const BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 1920,
    h: 1040,
};
const RESTORED: Rect = Rect {
    x: 120,
    y: 120,
    w: 800,
    h: 600,
};
const SIBLING: Rect = Rect {
    x: 0,
    y: 0,
    w: 800,
    h: 600,
};

fn sibling_key() -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd: 100,
        pid: 8,
        creation: "creation-sib".to_owned(),
    }
}

fn observed_normal(
    hwnd: u64,
    token: &str,
    frame: Rect,
    key: &crate::workspace::WindowKey,
) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: "C:\\test\\app.exe".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }
}

fn areas_for(output: &str) -> Vec<super::MonitorArea> {
    vec![super::MonitorArea {
        device: output.to_owned(),
        work: BOUNDS,
        full: BOUNDS,
    }]
}

fn snap_origin(key: &crate::workspace::WindowKey, token: &str) -> crate::snapkey::SnapOrigin {
    crate::snapkey::SnapOrigin {
        hwnd: key.hwnd,
        token: token.to_owned(),
        pid: key.pid,
        creation: key.creation.clone(),
    }
}

fn engine_float(state: &super::TileLoop, output: &str, active: &str, token: &str) -> bool {
    use tiler_core::directional::WindowId;
    let (_, key) =
        crate::workspace_owner::workspace_domain(output, active, BOUNDS, crate::tiling::INNER_GAP);
    state
        .engine
        .session(&key)
        .is_some_and(|s| s.is_exception(&WindowId(token.to_owned())))
}

fn engine_tiled(state: &super::TileLoop, output: &str, active: &str, token: &str) -> bool {
    use tiler_core::directional::WindowId;
    let (_, key) =
        crate::workspace_owner::workspace_domain(output, active, BOUNDS, crate::tiling::INNER_GAP);
    state.engine.session(&key).is_some_and(|s| {
        s.snapshot().windows.iter().any(|l| l.window.0 == token)
            && !s.is_exception(&WindowId(token.to_owned()))
    })
}

/// Production domain constructor for one output/active pair: the same
/// outer-gap-inset bounds `prepare_toggle_float` resolves, so staged
/// Engine sessions match the admission domain exactly (the Engine binds
/// sessions by bounds/gap, not just output/workspace).
fn domain_for(
    state: &super::TileLoop,
    output: &str,
    active: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    let areas = areas_for(output);
    super::workspace_domain_for(output, active, &areas, state.inner_gap, state.outer_gap)
        .expect("domain resolves")
}

/// Reconcile two tiles through production assembly plus the stamped
/// Engine handle; returns the committed rows.
fn commit_two_tiles(
    state: &mut super::TileLoop,
    output: &str,
    active: &str,
    observed: &[super::ObservedWindow],
    focus: &str,
    correlation: &str,
) -> Vec<crate::workspace_owner::OwnerRow> {
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    use tiler_core::size_hints::WindowSizeHints;
    let (domain, domain_key) = domain_for(state, output, active);
    let mut hint_cx = super::HintCx::new();
    let rows = super::assemble_domain_rows(
        state,
        output,
        active,
        observed,
        &[],
        "b9-test",
        correlation,
        &mut hint_cx,
    )
    .expect("rows assemble");
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse(correlation).expect("correlation"),
        super::revision_for(state, output, active),
        fp,
        &domain,
        &domain_key,
        crate::tiling::OUTER_GAP,
        &windows,
        Some(&WindowId(focus.to_owned())),
    );
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        matches!(reply, CoreReply::Tiled(_) | CoreReply::Projection(_)),
        "commit converges, got {reply:?}"
    );
    rows
}

/// Float one member through the production stamped handle.
fn commit_toggle_float(
    state: &mut super::TileLoop,
    output: &str,
    active: &str,
    rows: &[crate::workspace_owner::OwnerRow],
    focus: &str,
    window: &str,
    correlation: &str,
) {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    use tiler_core::size_hints::WindowSizeHints;
    let (domain, domain_key) = domain_for(state, output, active);
    let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    let revision = state
        .engine
        .session(&domain_key)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse(correlation).expect("correlation"),
        revision,
        fp,
        &domain,
        &domain_key,
        crate::tiling::OUTER_GAP,
        &carried,
        Some(&WindowId(focus.to_owned())),
    );
    event.command = CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    };
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "toggle converges, got {reply:?}"
    );
}

/// Shared staging for the clear-then-admit tests: two tiles with w8
/// floated through the real Engine. Returns output, active, w8 key,
/// sibling key.
fn stage_floating_pair() -> (
    super::TileLoop,
    String,
    String,
    crate::workspace::WindowKey,
    crate::workspace::WindowKey,
) {
    super::clear_test_hints();
    let mut state = test_state();
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    let key = float_key(101);
    let sib = sibling_key();
    stage_member(&mut state, &output, &active, &key, "w8");
    stage_member(&mut state, &output, &active, &sib, "w7");
    let observed = vec![
        observed_normal(101, "w8", RESTORED, &key),
        observed_normal(100, "w7", SIBLING, &sib),
    ];
    let rows = commit_two_tiles(&mut state, &output, &active, &observed, "w7", "b9-stage");
    commit_toggle_float(&mut state, &output, &active, &rows, "w7", "w8", "b9-float");
    assert!(
        engine_float(&state, &output, &active, "w8"),
        "staged ordinary float"
    );
    state.floated.insert(key.clone());
    (state, output, active, key, sib)
}

fn fresh_restored(
    key: &crate::workspace::WindowKey,
    sib: &crate::workspace::WindowKey,
) -> super::FreshFloatObservation {
    super::FreshFloatObservation {
        observed: vec![
            observed_normal(101, "w8", RESTORED, key),
            observed_normal(100, "w7", SIBLING, sib),
        ],
        retained: Vec::new(),
        skipped: Vec::new(),
    }
}

#[test]
fn cleared_float_admits_tiled_through_real_engine() {
    // HIGH: production ordering clear -> fresh observe -> existing
    // admission end to end with the real Engine and scripted native
    // effects (no mirrors). The stub call log pins the order; the fresh
    // observation carries the restored frame; the settled unfloat commits
    // a real tiled admission with the runtime (non-durable) override.
    // Geometry writes on invalid handles fail closed after the commit.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let fresh = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect("scripted clear succeeds");
    assert_eq!(
        script.calls,
        vec![
            "revalidate",
            "foreground",
            "suspend",
            "elevated",
            "meta",
            "restore",
            "pid",
            "observe",
            "foreground",
            "meta"
        ],
        "clear issues before the fresh observe; post guards re-read"
    );
    assert!(
        fresh
            .observed
            .iter()
            .any(|w| w.token == "w8" && w.visible == RESTORED),
        "admission below runs on the restored frame"
    );
    let origin = snap_origin(&key, "w8");
    let outcomes = std::cell::RefCell::new(Vec::new());
    let settle = |outcome: &'static str| {
        outcomes.borrow_mut().push(outcome.to_owned());
        serde_json::json!({"event": "float-toggle", "outcome": outcome})
    };
    let areas = areas_for(&output);
    let correlation2 = state.correlation();
    super::apply_unfloat_to_tiled(
        &mut state,
        &float_me(),
        &[BOUNDS],
        &areas,
        &fresh.observed,
        &fresh.retained,
        fresh.skipped,
        &key,
        "w8",
        &loc,
        Some(RESTORED),
        &origin,
        &settle,
        &log_path,
        2,
        &correlation2,
        "float",
        "unfloated",
        false,
    );
    assert!(
        engine_tiled(&state, &output, &active, "w8"),
        "fresh admission tiles the cleared float: {:?}",
        outcomes.borrow()
    );
    assert!(
        engine_tiled(&state, &output, &active, "w7"),
        "sibling survives"
    );
    assert!(
        !state.floated.contains(&key),
        "settled unfloat drops float intent"
    );
    assert!(
        state.fixed_tile_overrides.contains(&key),
        "explicit win records runtime suppress"
    );
    assert!(
        !state.fixed_origin_overrides.contains(&key),
        "generic unfloat installs no durable marker"
    );
    let outcomes = outcomes.borrow();
    assert_eq!(
        outcomes.as_slice(),
        ["unfloat-applied"],
        "settled unfloat applies: the Engine commits first, then geometry \
         finds planned == visible (no native write needed on these frames); \
         invalid handles take no writes: {outcomes:?}"
    );
}

#[test]
fn dispatched_clear_refuses_and_later_press_recovers() {
    // HIGH race: async completion still pending at observe time refuses
    // (still-maximized) with no admission; a later press with an observed
    // clear succeeds.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let pending = super::FreshFloatObservation {
        observed: Vec::new(),
        retained: vec![maximized_retained(&key, "w8")],
        skipped: Vec::new(),
    };
    let mut script = ScriptedClear::passing(101, pending);
    script.restore = "dispatched";
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("pending async completion refuses");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("still-maximized"))
    );
    assert!(
        engine_float(&state, &output, &active, "w8"),
        "refused toggle admits nothing"
    );
    assert!(state.floated.contains(&key));
    assert!(!state.fixed_tile_overrides.contains(&key));
    // Later press observes the completed clear and proceeds.
    let mut script2 = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    let correlation2 = state.correlation();
    let fresh = super::unfloat_maximize_clear(
        &mut state,
        &mut script2,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-2",
        &log_path,
        2,
        &correlation2,
    )
    .expect("later press observes the clear");
    assert!(
        fresh
            .observed
            .iter()
            .any(|w| w.token == "w8" && w.visible == RESTORED)
    );
}

#[test]
fn refocus_between_clear_and_observe_refuses() {
    // HIGH race: focus leaving the subject after the clear ends the
    // toggle with no admission; the float stays for a later press.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.foreground_post = 100;
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("refocused subject refuses");
    assert_eq!(err, ("float-refused-maximize-clear", Some("focus-raced")));
    assert!(
        engine_float(&state, &output, &active, "w8"),
        "refused toggle admits nothing"
    );
    assert!(state.floated.contains(&key));
}

#[test]
fn domain_move_between_clear_and_observe_refuses() {
    // HIGH race: the member leaving the domain after the clear ends the
    // toggle; pre-clear topology never drives admission.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let other = state
        .workspaces
        .workspace_ids(&output)
        .into_iter()
        .find(|id| id != &active)
        .expect("second workspace");
    let retained = vec![maximized_retained(&key, "w8")];
    // Eligible fresh frame so evaluation reaches the domain fence rather
    // than stopping at still-maximized: the move races closed here.
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.relocate_post_clear = Some((key.clone(), output.clone(), other.clone()));
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("moved member refuses");
    assert_eq!(err, ("float-refused-maximize-clear", Some("domain-raced")));
    assert!(
        engine_float(&state, &output, &active, "w8"),
        "refused toggle admits nothing"
    );
}

#[test]
fn recycled_handle_between_clear_and_observe_refuses() {
    // HIGH race: pid turnover after the clear ends the toggle.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.pid_current = false;
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("recycled handle refuses");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("identity-raced"))
    );
    assert!(
        engine_float(&state, &output, &active, "w8"),
        "refused toggle admits nothing"
    );
    assert!(state.floated.contains(&key));
}

#[test]
fn owned_fullscreen_transition_post_clear_refuses() {
    // MEDIUM: an owned fullscreen transition landing between the clear
    // and the fresh observe refuses even when the restored frame reads
    // eligible — post-clear re-reads owned metadata plus the fresh
    // captionless flag, same fence as pre-clear.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let log_path = state.log_path.clone();
    let loc = float_loc(&output, &active);
    // Owned metadata appears post-clear.
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_post = Some(super::FullscreenMeta {
        changed: 0,
        was_maximized: true,
        frame: None,
    });
    let correlation = state.correlation();
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("owned transition refuses");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(
        engine_float(&state, &output, &active, "w8"),
        "refused toggle admits nothing"
    );
    // Fresh captionless flag without metadata refuses the same way.
    let mut captionless = observed_normal(101, "w8", RESTORED, &key);
    captionless.facts.captionless_fullscreen = true;
    let mut script2 = ScriptedClear::passing(
        101,
        super::FreshFloatObservation {
            observed: vec![captionless, observed_normal(100, "w7", SIBLING, &sib)],
            retained: Vec::new(),
            skipped: Vec::new(),
        },
    );
    let correlation2 = state.correlation();
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script2,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-2",
        &log_path,
        2,
        &correlation2,
    )
    .expect_err("captionless fresh frame refuses");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(engine_float(&state, &output, &active, "w8"));
}

#[test]
fn retry_after_metadata_absence_proves_fresh_admission() {
    // Owned-metadata veto is per-attempt, not sticky: metadata present
    // post-clear refuses through the production B9 route, while a later
    // press with metadata absent and a cleared frame promotes the fresh
    // observation and settles a real tiled admission. This pins retry
    // behavior only; it does not test production metadata retirement
    // (only `exit_fullscreen_owned` clears the live props, which no
    // hermetic fixture drives).
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let log_path = state.log_path.clone();
    let loc = float_loc(&output, &active);
    let mut refusing = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    refusing.meta_post = Some(super::FullscreenMeta {
        changed: 0,
        was_maximized: true,
        frame: None,
    });
    let correlation = state.correlation();
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut refusing,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("owned transition refuses");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(engine_float(&state, &output, &active, "w8"));
    // Later press with metadata absent and a cleared frame: the route
    // promotes the fresh observation, and the settled unfloat commits a
    // real tiled admission with the runtime (non-durable) override.
    let mut retry = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    retry.meta_pre = None;
    retry.meta_post = None;
    let correlation2 = state.correlation();
    let fresh = super::unfloat_maximize_clear(
        &mut state,
        &mut retry,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-2",
        &log_path,
        2,
        &correlation2,
    )
    .expect("metadata absence promotes the cleared frame");
    assert!(
        fresh
            .observed
            .iter()
            .any(|w| w.token == "w8" && w.visible == RESTORED),
        "retry admits the restored frame"
    );
    let origin = snap_origin(&key, "w8");
    let outcomes = std::cell::RefCell::new(Vec::new());
    let settle = |outcome: &'static str| {
        outcomes.borrow_mut().push(outcome.to_owned());
        serde_json::json!({"event": "float-toggle", "outcome": outcome})
    };
    let areas = areas_for(&output);
    let correlation3 = state.correlation();
    super::apply_unfloat_to_tiled(
        &mut state,
        &float_me(),
        &[BOUNDS],
        &areas,
        &fresh.observed,
        &fresh.retained,
        fresh.skipped,
        &key,
        "w8",
        &loc,
        Some(RESTORED),
        &origin,
        &settle,
        &log_path,
        3,
        &correlation3,
        "float",
        "unfloated",
        false,
    );
    assert!(
        engine_tiled(&state, &output, &active, "w8"),
        "retry settles a real tiled admission: {:?}",
        outcomes.borrow()
    );
    assert!(
        !state.floated.contains(&key),
        "settled unfloat drops float intent"
    );
    assert_eq!(
        outcomes.borrow().as_slice(),
        ["unfloat-applied"],
        "settled retry applies: {outcomes:?}"
    );
}

#[test]
fn stale_restored_marker_retires_through_production_ordering() {
    // Stale owned marker whose chrome provably restored plus a live
    // no-cover probe retires via the production B9 ordering (metadata
    // removal only, no geometry write before the veto) and the cleared
    // frame promotes to a real tiled admission.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    });
    script.meta_post = None;
    script.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    script.retire_fullscreen = Some(false);
    script.retire_clear_ok = true;
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let fresh = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect("stale restored marker must retire and promote");
    assert_eq!(
        script.calls,
        vec![
            "revalidate",
            "foreground",
            "suspend",
            "elevated",
            "meta",
            "pid",
            "retire-probe",
            "revalidate",
            "meta",
            "retire-probe",
            "pid",
            "retire-clear",
            "meta",
            "restore",
            "pid",
            "observe",
            "foreground",
            "meta"
        ],
        "shared retirement (fence, proof, second lifetime pin, equality \
         re-read, second live proof, removal-only clear, verify) runs \
         before the maximize clear; the post fence re-reads"
    );
    assert!(
        fresh
            .observed
            .iter()
            .any(|w| w.token == "w8" && w.visible == RESTORED),
        "retired route promotes the restored frame"
    );
    let origin = snap_origin(&key, "w8");
    let outcomes = std::cell::RefCell::new(Vec::new());
    let settle = |outcome: &'static str| {
        outcomes.borrow_mut().push(outcome.to_owned());
        serde_json::json!({"event": "float-toggle", "outcome": outcome})
    };
    let areas = areas_for(&output);
    let correlation2 = state.correlation();
    super::apply_unfloat_to_tiled(
        &mut state,
        &float_me(),
        &[BOUNDS],
        &areas,
        &fresh.observed,
        &fresh.retained,
        fresh.skipped,
        &key,
        "w8",
        &loc,
        Some(RESTORED),
        &origin,
        &settle,
        &log_path,
        2,
        &correlation2,
        "float",
        "unfloated",
        false,
    );
    assert!(
        engine_tiled(&state, &output, &active, "w8"),
        "retired stale marker settles a real admission: {:?}",
        outcomes.borrow()
    );
}

#[test]
fn captionless_noncover_transition_keeps_veto_without_removal() {
    // Owned captionless frame that merely stopped covering (style still
    // cleared) is the existing veto purpose: no removal, still vetoed,
    // no maximize clear attempted.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    });
    script.meta_post = script.meta_pre;
    script.retire_style = Some(0);
    script.retire_fullscreen = Some(false);
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("transitional non-cover must still veto");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(
        !script.calls.contains(&"retire-clear"),
        "transitional geometry never removes the marker: {:?}",
        script.calls
    );
    assert!(
        !script.calls.contains(&"restore"),
        "vetoed route issues no maximize clear: {:?}",
        script.calls
    );
    assert!(engine_float(&state, &output, &active, "w8"));
}

#[test]
fn live_cover_and_unreadable_and_identity_keep_veto() {
    // Real cover, unreadable style/frame, and recycled-HWND identity all
    // fail closed: no removal, still vetoed.
    for (name, style, fullscreen, pid_ok) in [
        ("cover", Some(0), Some(true), true),
        ("unreadable-style", None, Some(false), true),
        (
            "unreadable-frame",
            Some(super::FULLSCREEN_STYLE_BITS),
            None,
            true,
        ),
        (
            "identity-raced",
            Some(super::FULLSCREEN_STYLE_BITS),
            Some(false),
            false,
        ),
    ] {
        let (mut state, output, active, key, sib) = stage_floating_pair();
        let retained = vec![maximized_retained(&key, "w8")];
        let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
        script.meta_pre = Some(super::FullscreenMeta {
            changed: super::FULLSCREEN_STYLE_BITS,
            was_maximized: false,
            frame: None,
        });
        script.meta_post = script.meta_pre;
        script.retire_style = style;
        script.retire_fullscreen = fullscreen;
        script.pid_current = pid_ok;
        let log_path = state.log_path.clone();
        let correlation = state.correlation();
        let loc = float_loc(&output, &active);
        let err = super::unfloat_maximize_clear(
            &mut state,
            &mut script,
            &float_me(),
            &[BOUNDS],
            &[],
            &retained,
            &key,
            "w8",
            &loc,
            "origin-1",
            &log_path,
            1,
            &correlation,
        )
        .expect_err("must fail closed");
        assert_eq!(
            err,
            ("float-refused-maximize-clear", Some("fullscreen-raced")),
            "{name} must still veto"
        );
        assert!(
            !script.calls.contains(&"retire-clear"),
            "{name} never removes the marker: {:?}",
            script.calls
        );
        if !pid_ok {
            assert!(
                !script.calls.contains(&"retire-probe"),
                "recycled identity probes nothing: {:?}",
                script.calls
            );
        }
        assert!(
            engine_float(&state, &output, &active, "w8"),
            "{name} admits nothing"
        );
    }
}

#[test]
fn zero_changed_and_failed_removal_keep_veto() {
    // Originally captionless entry (changed == 0) is indistinguishable
    // from a captionless non-cover transition: never retires. A failed
    // removal readback also keeps the veto for a later press.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(super::FullscreenMeta {
        changed: 0,
        was_maximized: false,
        frame: None,
    });
    script.meta_post = script.meta_pre;
    script.retire_style = Some(0);
    script.retire_fullscreen = Some(false);
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("zero-bit preimage never retires");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(!script.calls.contains(&"retire-clear"));
    // Eligible proof with a failed removal still vetoes.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    });
    script.meta_post = script.meta_pre;
    script.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    script.retire_fullscreen = Some(false);
    script.retire_clear_ok = false;
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("failed removal keeps the veto");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(script.calls.contains(&"retire-clear"));
}

#[test]
fn stale_retire_pure_gate_pins_proof_matrix() {
    use crate::tiling::stale_fullscreen_retire_eligible as eligible;
    assert!(eligible(super::FULLSCREEN_STYLE_BITS, true, Some(false)));
    assert!(!eligible(super::FULLSCREEN_STYLE_BITS, false, Some(false)));
    assert!(!eligible(super::FULLSCREEN_STYLE_BITS, true, Some(true)));
    assert!(!eligible(super::FULLSCREEN_STYLE_BITS, true, None));
    assert!(!eligible(super::FULLSCREEN_STYLE_BITS, false, None));
    assert!(!eligible(0, true, Some(false)));
    assert!(!eligible(0, false, Some(false)));
    assert!(!eligible(0, true, None));
}

#[test]
fn retirement_race_after_proof_keeps_veto_without_removal() {
    // Races landing after the first proof must not delete: a changed
    // marker, a re-fullscreen second proof, and a member-tag change
    // (second revalidation fails, covering same-PID HWND reuse) all keep
    // the veto with no removal.
    let stale = || super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    };
    // Metadata changed between proof and pin.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(stale());
    script.meta_cur_set = true;
    script.meta_cur_val = Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: true,
        frame: None,
    });
    script.meta_post = script.meta_cur_val;
    script.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    script.retire_fullscreen = Some(false);
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("changed marker keeps the veto");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(
        !script.calls.contains(&"retire-clear"),
        "changed preimage removes nothing"
    );
    assert!(!script.calls.contains(&"restore"));
    // Genuine re-fullscreen racing the pin: second live proof covers.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(stale());
    script.meta_post = script.meta_pre;
    script.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    script.retire_fullscreen = Some(false);
    script.retire_probe2_set = true;
    script.retire_style2 = Some(0);
    script.retire_fullscreen2 = Some(true);
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("re-fullscreen race keeps the veto");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(
        !script.calls.contains(&"retire-clear"),
        "re-fullscreen removes nothing"
    );
    // Member-tag change / same-PID reuse: second revalidation fails.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(stale());
    script.meta_post = script.meta_pre;
    script.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    script.retire_fullscreen = Some(false);
    script.revalidate_fail_second = true;
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("tag drift keeps the veto");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(
        !script.calls.contains(&"retire-clear"),
        "drifted lifetime removes nothing"
    );
    assert!(engine_float(&state, &output, &active, "w8"));
}

#[test]
fn retirement_guards_block_mutation_before_proof() {
    // A failing home/focus/suspend/elevation guard authorizes no removal:
    // the sequence returns before any retire probe or clear.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let mut script = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    script.meta_pre = Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    });
    script.meta_post = script.meta_pre;
    script.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    script.retire_fullscreen = Some(false);
    script.suspend_block = true;
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect_err("suspended guard blocks before retirement");
    // Fullscreen precedence wins over suspension in the shared gate, so
    // the veto reads fullscreen-raced; the point below is that the
    // guarded route probed and removed nothing.
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(
        !script.calls.contains(&"retire-probe"),
        "guarded route probes nothing"
    );
    assert!(
        !script.calls.contains(&"retire-clear"),
        "guarded route removes nothing"
    );
    assert!(engine_float(&state, &output, &active, "w8"));
}

#[test]
fn refullscreen_after_completed_retirement_stays_protected() {
    // A genuine re-fullscreen after a completed retirement mints a fresh
    // marker that the next press protects: live cover keeps the veto with
    // no removal.
    let (mut state, output, active, key, sib) = stage_floating_pair();
    let retained = vec![maximized_retained(&key, "w8")];
    let log_path = state.log_path.clone();
    let loc = float_loc(&output, &active);
    let mut first = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    first.meta_pre = Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    });
    first.meta_post = None;
    first.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    first.retire_fullscreen = Some(false);
    let correlation = state.correlation();
    super::unfloat_maximize_clear(
        &mut state,
        &mut first,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect("first press retires the stale marker");
    // New owned marker from a genuine re-fullscreen.
    let mut second = ScriptedClear::passing(101, fresh_restored(&key, &sib));
    second.meta_pre = Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    });
    second.meta_post = second.meta_pre;
    second.retire_style = Some(0);
    second.retire_fullscreen = Some(true);
    let correlation2 = state.correlation();
    let err = super::unfloat_maximize_clear(
        &mut state,
        &mut second,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "w8",
        &loc,
        "origin-2",
        &log_path,
        2,
        &correlation2,
    )
    .expect_err("fresh fullscreen marker stays vetoed");
    assert_eq!(
        err,
        ("float-refused-maximize-clear", Some("fullscreen-raced"))
    );
    assert!(
        !second.calls.contains(&"retire-clear"),
        "new marker is never removed"
    );
}

fn sweep_state() -> (super::TileLoop, crate::workspace::WindowKey) {
    super::clear_test_hints();
    let mut state = test_state();
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    let key = float_key(101);
    stage_member(&mut state, &output, &active, &key, "w8");
    (state, key)
}

fn empty_fresh() -> super::FreshFloatObservation {
    super::FreshFloatObservation {
        observed: Vec::new(),
        retained: Vec::new(),
        skipped: Vec::new(),
    }
}

fn sweep_script(marker: Option<super::FullscreenMeta>) -> ScriptedClear {
    let mut script = ScriptedClear::passing(101, empty_fresh());
    script.meta_pre = marker;
    script.meta_post = marker;
    script.retire_style = Some(super::FULLSCREEN_STYLE_BITS);
    script.retire_fullscreen = Some(false);
    script
}

#[test]
fn sweep_retires_restored_marker_without_explicit_action() {
    // Automatic retirement needs no move/unfloat press: a verified normal
    // member whose chrome provably restored plus a live no-cover probe
    // runs the shared sequence (metadata removal only) and drops the
    // marker.
    let (mut state, key) = sweep_state();
    let observed = vec![observed_normal(101, "w8", RESTORED, &key)];
    let retained = Vec::new();
    let mut script = sweep_script(Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    }));
    super::sweep_retire_stale_fullscreen(
        &mut script,
        &mut state,
        &float_me(),
        &observed,
        &retained,
        &[BOUNDS],
    );
    assert_eq!(
        script.calls,
        vec![
            "suspend",
            "elevated",
            "meta",
            "pid",
            "retire-probe",
            "revalidate",
            "meta",
            "retire-probe",
            "pid",
            "retire-clear",
            "meta"
        ],
        "sweep runs the shared ordering with no explicit action"
    );
    assert!(script.retire_clear_called, "restored marker is removed");
}

#[test]
fn sweep_retires_restored_marker_on_intentional_float() {
    // Float intent does not shield a positively restored marker: the
    // sweep retires it while leaving float membership for the next
    // toggle to admit.
    let (mut state, key) = sweep_state();
    state.floated.insert(key.clone());
    let observed = vec![observed_normal(101, "w8", RESTORED, &key)];
    let retained = Vec::new();
    let mut script = sweep_script(Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    }));
    super::sweep_retire_stale_fullscreen(
        &mut script,
        &mut state,
        &float_me(),
        &observed,
        &retained,
        &[BOUNDS],
    );
    assert!(script.retire_clear_called, "float member marker is removed");
    assert!(
        state.floated.contains(&key),
        "sweep leaves float intent untouched"
    );
}

#[test]
fn sweep_transient_and_unreadable_keep_marker() {
    // A captionless non-cover transition plus unreadable style/frame all
    // fail closed with no removal.
    for (name, style, fullscreen) in [
        ("transient", Some(0), Some(false)),
        ("unreadable-style", None, Some(false)),
        ("unreadable-frame", Some(super::FULLSCREEN_STYLE_BITS), None),
    ] {
        let (mut state, key) = sweep_state();
        let observed = vec![observed_normal(101, "w8", RESTORED, &key)];
        let retained = Vec::new();
        let mut script = sweep_script(Some(super::FullscreenMeta {
            changed: super::FULLSCREEN_STYLE_BITS,
            was_maximized: false,
            frame: None,
        }));
        script.retire_style = style;
        script.retire_fullscreen = fullscreen;
        super::sweep_retire_stale_fullscreen(
            &mut script,
            &mut state,
            &float_me(),
            &observed,
            &retained,
            &[BOUNDS],
        );
        assert!(
            !script.retire_clear_called,
            "{name} removes nothing: {:?}",
            script.calls
        );
    }
    // Absent markers probe nothing.
    let (mut state, key) = sweep_state();
    let observed = vec![observed_normal(101, "w8", RESTORED, &key)];
    let retained = Vec::new();
    let mut script = sweep_script(None);
    super::sweep_retire_stale_fullscreen(
        &mut script,
        &mut state,
        &float_me(),
        &observed,
        &retained,
        &[BOUNDS],
    );
    assert!(!script.calls.contains(&"retire-probe"));
    assert!(!script.retire_clear_called);
}

#[test]
fn sweep_guards_and_lifetime_block_mutation() {
    // Suspended, elevated, and recycled-identity sweeps mutate nothing:
    // no probe and no removal.
    let (mut state, key) = sweep_state();
    let observed = vec![observed_normal(101, "w8", RESTORED, &key)];
    let retained: Vec<super::RetainedRow> = Vec::new();
    let mut script = sweep_script(Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    }));
    script.suspend_block = true;
    super::sweep_retire_stale_fullscreen(
        &mut script,
        &mut state,
        &float_me(),
        &observed,
        &retained,
        &[BOUNDS],
    );
    assert_eq!(script.calls, vec!["suspend"]);
    let mut script = sweep_script(Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    }));
    script.elevated = true;
    super::sweep_retire_stale_fullscreen(
        &mut script,
        &mut state,
        &float_me(),
        &observed,
        &retained,
        &[BOUNDS],
    );
    assert_eq!(script.calls, vec!["suspend", "elevated"]);
    let mut script = sweep_script(Some(super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    }));
    script.pid_current = false;
    super::sweep_retire_stale_fullscreen(
        &mut script,
        &mut state,
        &float_me(),
        &observed,
        &retained,
        &[BOUNDS],
    );
    assert_eq!(script.calls, vec!["suspend", "elevated", "meta", "pid"]);
    assert!(!script.calls.contains(&"retire-probe"));
    assert!(!script.retire_clear_called);
}

#[test]
fn fixed_automatic_float_clears_then_records_durable_override() {
    // D3/D7 user-win: a maximized fixed-automatic float takes the B9
    // clear, then the settled unfloat tiles with the durable
    // fixed-origin override. Marker install on invalid handles degrades
    // to the log (real marker IO rides the owned-window fixtures); the
    // runtime sets below are the persistence contract.
    super::clear_test_hints();
    let mut state = test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    let key = crate::workspace::WindowKey {
        hwnd: 2101,
        pid: 9,
        creation: "creation-fix".to_owned(),
    };
    let sib = crate::workspace::WindowKey {
        hwnd: 2100,
        pid: 8,
        creation: "creation-sib2".to_owned(),
    };
    stage_member(&mut state, &output, &active, &key, "wfix");
    stage_member(&mut state, &output, &active, &sib, "w1");
    let fixed_hints = crate::tiling::hints_from_outer(
        Some((656, 496)),
        Some((656, 496)),
        crate::tiling::FrameInsets {
            left: 8,
            top: 8,
            right: 8,
            bottom: 8,
        },
    );
    super::inject_test_hints(2101, fixed_hints);
    let observed = vec![
        observed_normal(2101, "wfix", SIBLING, &key),
        observed_normal(2100, "w1", SIBLING, &sib),
    ];
    commit_two_tiles(
        &mut state,
        &output,
        &active,
        &observed,
        "w1",
        "b9-fix-stage",
    );
    let (_, domain_key) = crate::workspace_owner::workspace_domain(
        output.as_str(),
        active.as_str(),
        BOUNDS,
        crate::tiling::INNER_GAP,
    );
    assert!(
        state
            .engine
            .session(&domain_key)
            .is_some_and(|s| s
                .is_automatic_fixed_float(&tiler_core::directional::WindowId("wfix".to_owned()))),
        "fixed hints classify automatic on admission"
    );
    let retained = vec![maximized_retained(&key, "wfix")];
    let fresh_observed = vec![
        observed_normal(2101, "wfix", RESTORED, &key),
        observed_normal(2100, "w1", SIBLING, &sib),
    ];
    let mut script = ScriptedClear::passing(
        2101,
        super::FreshFloatObservation {
            observed: fresh_observed,
            retained: Vec::new(),
            skipped: Vec::new(),
        },
    );
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    let loc = float_loc(&output, &active);
    let fresh = super::unfloat_maximize_clear(
        &mut state,
        &mut script,
        &float_me(),
        &[BOUNDS],
        &[],
        &retained,
        &key,
        "wfix",
        &loc,
        "origin-1",
        &log_path,
        1,
        &correlation,
    )
    .expect("fixed clear succeeds");
    assert!(
        fresh
            .observed
            .iter()
            .any(|w| w.token == "wfix" && w.visible == RESTORED)
    );
    let origin = snap_origin(&key, "wfix");
    let settle = |_: &'static str| serde_json::json!({"event": "float-toggle"});
    let areas = areas_for(&output);
    let correlation2 = state.correlation();
    super::apply_unfloat_to_tiled(
        &mut state,
        &float_me(),
        &[BOUNDS],
        &areas,
        &fresh.observed,
        &fresh.retained,
        fresh.skipped,
        &key,
        "wfix",
        &loc,
        Some(RESTORED),
        &origin,
        &settle,
        &log_path,
        2,
        &correlation2,
        "float",
        "unfloated",
        false,
    );
    assert!(
        engine_tiled(&state, &output, &active, "wfix"),
        "explicit tile wins over automatic"
    );
    assert!(
        state.fixed_tile_overrides.contains(&key),
        "user win records runtime suppress"
    );
    assert!(
        state.fixed_origin_overrides.contains(&key),
        "fixed origin records the durable D7 override"
    );
    super::clear_test_hints();
}
