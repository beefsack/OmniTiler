#![forbid(unsafe_code)]
// Windows-only bounded per-window diagnostics (`window-diag` normal event).
//
// Integration shape: every leg drives the production row assembly (real hint
// queries through the injected fresh-outcome seam, real last-good fallback,
// real hint-lane staging) and, where geometry joins, the production Engine
// commit for real planned rectangles. Only the native setter/readback facts
// (attempted writes, outer targets, post rectangles, skip lists) are
// scripted, fed through the production `stage_write_samples` seam that
// `apply_geometry` itself calls, and flushed through the production
// throttle/dedupe/emission path with a pinned clock. No tiler launches, no
// user windows, no input/hooks/registry; all HWNDs are synthetic and every
// native class read is injected.
use super::rmax03_adapter_tests::test_state;
use super::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tiler_core::boundary::{CoreCommand, CoreReply};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;
use tiler_core::size_hints::WindowSizeHints;

const DW_EXE: &str = "C:\\Apps\\FlapApp\\flap.exe";

/// One synthetic visible member with distinct fake identity per token, a full
/// Windows-style exe path (basename extraction is under test), and a
/// scripted lifetime tag.
fn dw_member(
    state: &mut TileLoop,
    output: &str,
    ws: &str,
    token: &str,
    hwnd: u64,
) -> crate::workspace::WindowKey {
    let key = crate::workspace::WindowKey {
        hwnd,
        pid: 9000 + hwnd as u32,
        creation: format!("cdw{hwnd:016x}"),
    };
    assert!(state.workspaces.assign(key.clone(), output, ws, false));
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-dw".to_owned(),
            session_id: 1,
            exe_path: DW_EXE.to_owned(),
        },
    );
    state
        .member_tags
        .insert(key.clone(), format!("dw-tag-{token}"));
    state.last_hwnds.insert(key.hwnd);
    inject_pending_tag(key.hwnd, Some(format!("dw-tag-{token}")));
    key
}

fn dw_domain(
    output: &str,
    ws: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    crate::workspace_owner::workspace_domain(
        output,
        ws,
        Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        },
        8,
    )
}

fn dw_observed(token: &str, key: &crate::workspace::WindowKey, rect: Rect) -> ObservedWindow {
    ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: DW_EXE.to_owned(),
            user_sid: "S-1-5-dw".to_owned(),
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

/// Synthetic converted hints through the production conversion path (never
/// hand-built): outer track minus 8px frames on each axis.
fn dw_hints(min_outer: Option<(i32, i32)>) -> WindowSizeHints {
    crate::tiling::hints_from_outer(
        min_outer,
        None,
        crate::tiling::FrameInsets {
            left: 8,
            top: 8,
            right: 8,
            bottom: 8,
        },
    )
}

struct Dw {
    state: TileLoop,
    output: String,
    ws: String,
    domain: (
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    t0: Instant,
    now: Instant,
}

fn dw_setup(n: usize) -> Dw {
    clear_test_hints();
    clear_pending_seams();
    clear_diag_classes();
    clear_diag_pids();
    let t0 = Instant::now();
    inject_hint_now(Some(t0));
    let mut state = test_state();
    let output = "mon-dw".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let domain = dw_domain(&output, &ws);
    for i in 0..n {
        let token = format!("dw{:02}", i + 1);
        let hwnd = 8200 + i as u64;
        let key = dw_member(&mut state, &output, &ws, &token, hwnd);
        inject_diag_class(key.hwnd, &format!("DwClass{i}"));
        inject_diag_pid(key.hwnd, key.pid);
    }
    Dw {
        state,
        output,
        ws,
        domain,
        t0,
        now: t0,
    }
}

fn dw_teardown() {
    clear_test_hints();
    clear_pending_seams();
    clear_diag_classes();
    clear_diag_pids();
}

fn dw_keys(f: &Dw) -> Vec<crate::workspace::WindowKey> {
    let mut keys: Vec<_> = f.state.member_tokens.keys().cloned().collect();
    keys.sort_by_key(|k| k.hwnd);
    keys
}

fn dw_token(f: &Dw, key: &crate::workspace::WindowKey) -> String {
    f.state.member_tokens.get(key).cloned().expect("token")
}

/// Pin the harness clock (mirrors the production hint-clock seam the
/// throttle reads) and return it for explicit snapshot `now` values.
fn dw_advance(f: &mut Dw, ms: u64) -> Instant {
    f.now += Duration::from_millis(ms);
    inject_hint_now(Some(f.now));
    f.now
}

/// Fresh-hint leg setup: clear the outcome/hint seams (which also unpins the
/// clock), re-pin the harness time, then inject one fresh tick.
fn dw_fresh_at(f: &Dw, hints: &[(u64, WindowSizeHints)]) {
    clear_test_hints();
    inject_hint_now(Some(f.now));
    for (hwnd, h) in hints {
        inject_test_hints(*hwnd, *h);
    }
}

/// Scripted native-side facts for one apply pass, fed through the production
/// staging seam: which tokens the setter attempted, their outer targets and
/// skip reasons, and the readback rectangles.
#[derive(Default)]
struct DwNative {
    written: Vec<String>,
    skipped: Vec<(String, String)>,
    outer: Vec<(String, Rect)>,
    post: Vec<(String, Rect)>,
}

/// Production assembly plus Engine commit: real hint queries, real fallback,
/// real planned geometry. Returns rows plus the Engine's planned writes.
fn dw_assemble(
    f: &mut Dw,
    observed: &[ObservedWindow],
    correlation: &str,
) -> (
    Vec<crate::workspace_owner::OwnerRow>,
    Vec<crate::workspace_owner::PlannedWrite>,
) {
    f.state.tick += 1;
    for window in observed {
        f.state.overlay_now.insert(
            window.token.clone(),
            (window.facts.maximized, window.facts.captionless_fullscreen),
        );
    }
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut f.state,
        &f.output,
        &f.ws,
        observed,
        &[],
        "dw-test",
        correlation,
        &mut hint_cx,
    )
    .expect("rows assemble");
    let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|row| {
            (
                WindowId(row.token.clone()),
                row.rect,
                row.hints,
                row.floating,
            )
        })
        .collect();
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|row| (row.token.clone(), row.rect))
            .collect::<Vec<_>>(),
    );
    let focus = carried
        .last()
        .map(|(w, _, _, _)| w.clone())
        .expect("one row");
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &f.state.owner,
        &f.state.generation,
        &CorrelationId::parse(correlation).expect("correlation"),
        revision_for(&f.state, &f.output, &f.ws),
        fp,
        &f.domain.0,
        &f.domain.1,
        8,
        &carried,
        Some(&focus),
    );
    event.command = CoreCommand::Reconcile;
    let reply = handle_engine_event(&mut f.state, &mut event);
    assert!(
        matches!(reply, CoreReply::Tiled(_) | CoreReply::Projection(_)),
        "commit converges, got {reply:?}"
    );
    let desired = crate::workspace_owner::planned_writes(&reply).unwrap_or_default();
    (rows, desired)
}

/// Production write-lane staging over scripted native facts plus production
/// flush. The snapshot mirrors the production apply path, which clones the
/// refused tracker at pass entry.
fn dw_finish(
    f: &mut Dw,
    observed: &[ObservedWindow],
    desired: &[crate::workspace_owner::PlannedWrite],
    correlation: &str,
    native: DwNative,
) -> Option<serde_json::Value> {
    let snapshot = f.state.refused.clone();
    dw_finish_with(f, observed, desired, correlation, native, &snapshot)
}

/// [`dw_finish`] with an explicit pre-write refused-tracker snapshot, for
/// suppression/re-arm legs whose lane history the scripted native facts
/// imply (the staging seam never mutates the live tracker itself). Every
/// emitted entry must carry this leg's staging context (operation plus
/// correlation), never the flush context: entries are never relabeled.
fn dw_finish_with(
    f: &mut Dw,
    observed: &[ObservedWindow],
    desired: &[crate::workspace_owner::PlannedWrite],
    correlation: &str,
    native: DwNative,
    snapshot: &crate::tiling::RefusedTracker,
) -> Option<serde_json::Value> {
    let written: HashSet<String> = native.written.into_iter().collect();
    let outer: HashMap<String, Rect> = native.outer.into_iter().collect();
    let post: HashMap<String, Rect> = native.post.into_iter().collect();
    let now = f.now;
    stage_write_samples(
        &mut f.state,
        desired,
        &native.skipped,
        &written,
        &outer,
        observed,
        &post,
        snapshot,
        now,
    );
    let flushed = flush_window_diag(&mut f.state);
    if let Some(ref event) = flushed {
        for entry in event
            .get("entries")
            .expect("entries")
            .as_array()
            .expect("array")
        {
            assert_eq!(
                entry.get("op").and_then(|v| v.as_str()),
                Some("dw-test"),
                "entries keep staging operation"
            );
            assert_eq!(
                entry.get("correlation").and_then(|v| v.as_str()),
                Some(correlation),
                "entries keep staging correlation"
            );
        }
    }
    flushed
}

fn dw_planned(desired: &[crate::workspace_owner::PlannedWrite], token: &str) -> Rect {
    desired
        .iter()
        .find(|w| w.window.0 == token)
        .unwrap_or_else(|| panic!("plan for {token}"))
        .rect
}

fn dw_entry<'a>(event: &'a serde_json::Value, token: &str) -> &'a serde_json::Value {
    event
        .get("entries")
        .expect("entries")
        .as_array()
        .expect("array")
        .iter()
        .find(|e| e.get("window").and_then(|w| w.as_str()) == Some(token))
        .unwrap_or_else(|| panic!("entry for {token}"))
}

fn dw_rect(value: &serde_json::Value) -> Rect {
    let a = value.as_array().expect("rect array");
    Rect {
        x: a[0].as_i64().expect("x") as i32,
        y: a[1].as_i64().expect("y") as i32,
        w: a[2].as_i64().expect("w") as i32,
        h: a[3].as_i64().expect("h") as i32,
    }
}

fn dw_str<'a>(entry: &'a serde_json::Value, field: &str) -> &'a str {
    entry.get(field).and_then(|v| v.as_str()).expect(field)
}

const DW_STALE: Rect = Rect {
    x: 0,
    y: 0,
    w: 400,
    h: 300,
};

#[test]
fn healthy_converged_tick_emits_nothing_without_class_read() {
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    dw_fresh_at(&f, &[(keys[0].hwnd, dw_hints(Some((1016, 16))))]);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-h0");
    let planned = dw_planned(&desired, &token);
    // Converged write: post equals the request, so the sample is steady
    // state and must stay quiet (and resolve no class).
    let flushed = dw_finish(
        &mut f,
        &observed,
        &desired,
        "dw-h0",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    );
    assert!(flushed.is_none(), "converged tick stays quiet");
    assert!(
        f.state.diag_pending.contains_key(&token),
        "sample still staged"
    );
    assert!(
        diag_class_cached(&f.state, &token).is_none(),
        "quiet samples read no class"
    );
    dw_teardown();
}

#[test]
fn first_problem_emits_identity_geometry_and_privacy() {
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    let wide = dw_hints(Some((1016, 16)));
    assert_eq!(wide.min_w, Some(1000));
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-p0");
    let planned = dw_planned(&desired, &token);
    assert!(
        dw_finish(
            &mut f,
            &observed,
            &desired,
            "dw-p0",
            DwNative {
                written: vec![token.clone()],
                outer: vec![(token.clone(), planned)],
                post: vec![(token.clone(), planned)],
                ..DwNative::default()
            },
        )
        .is_none(),
        "healthy seed stays quiet"
    );
    // Timeout leg on a controlled clock: fallback reuses the seed hint, the
    // Engine plan must not move, and the write still mismatches.
    dw_advance(&mut f, 100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-p1");
    assert_eq!(desired_b, desired, "fallback holds targets");
    let report = f
        .state
        .hint_fallback_report(&token)
        .expect("fallback report");
    assert!(report.reused);
    assert_eq!(report.age_ms, 100);
    assert_eq!(report.reason, "reused");
    let flushed = dw_finish(
        &mut f,
        &observed,
        &desired_b,
        "dw-p1",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), DW_STALE)],
            ..DwNative::default()
        },
    );
    let event = flushed.expect("first problem emits");
    assert_eq!(dw_str(&event, "event"), "window-diag");
    assert_eq!(event.get("omitted").and_then(|v| v.as_u64()), Some(0));
    let entries = event
        .get("entries")
        .expect("entries")
        .as_array()
        .expect("array");
    assert_eq!(entries.len(), 1);
    let entry = dw_entry(&event, &token);
    assert_eq!(dw_str(entry, "class"), "DwClass0");
    assert_eq!(dw_str(entry, "exe"), "flap.exe");
    assert_eq!(dw_str(entry, "outcome"), "timeout");
    assert_eq!(entry.get("fallback").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        entry.get("fallback_age_ms").and_then(|v| v.as_u64()),
        Some(100)
    );
    assert_eq!(dw_str(entry, "fallback_reason"), "reused");
    // Injected stand-ins attempt no native query, so the actual cost reads
    // zero (see the native-fallthrough cost test for nonzero values).
    assert_eq!(entry.get("timeout_ms").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(entry.get("query_us").and_then(|v| v.as_u64()), Some(0));
    // Per-entry staging context rides the entry; the envelope carries none.
    assert_eq!(dw_str(entry, "op"), "dw-test");
    assert_eq!(dw_str(entry, "correlation"), "dw-p1");
    assert!(entry.get("tick").and_then(|v| v.as_u64()).is_some());
    assert!(event.get("op").is_none(), "no envelope operation");
    assert!(
        event.get("correlation").is_none(),
        "no envelope correlation"
    );
    let carried_min = entry.get("carried_min").expect("carried_min");
    assert_eq!(carried_min[0].as_i64(), Some(1000));
    assert!(carried_min[1].is_null());
    assert_eq!(dw_rect(entry.get("planned").expect("planned")), planned);
    assert_eq!(dw_rect(entry.get("effective").expect("effective")), planned);
    assert_eq!(dw_rect(entry.get("outer").expect("outer")), planned);
    assert_eq!(dw_rect(entry.get("pre").expect("pre")), DW_STALE);
    assert_eq!(dw_rect(entry.get("post").expect("post")), DW_STALE);
    assert_eq!(dw_str(entry, "disposition"), "wrote");
    assert_eq!(dw_str(entry, "suppress"), "none");
    assert!(entry.get("hwnd").is_none(), "no raw handle leaks");
    // Privacy: basenames and class only. Full paths, SIDs, creation
    // strings, lifetime tags, and raw HWND digits never serialize.
    let text = serde_json::to_string(&event).expect("json");
    assert!(text.contains("flap.exe"));
    assert!(!text.contains("C:\\Apps"), "full path leaks");
    assert!(!text.contains("C:/Apps"), "full path leaks");
    assert!(!text.contains("S-1-5"), "sid leaks");
    assert!(!text.contains("cdw"), "creation leaks");
    assert!(!text.contains("dw-tag"), "lifetime tag leaks");
    assert!(!text.contains("8200"), "hwnd leaks");
    dw_teardown();
}

#[test]
fn repeated_identical_flap_coalesces_and_delivers_latest() {
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    dw_fresh_at(&f, &[(keys[0].hwnd, dw_hints(Some((1016, 16))))]);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-t0");
    let planned = dw_planned(&desired, &token);
    let converged = DwNative {
        written: vec![token.clone()],
        outer: vec![(token.clone(), planned)],
        post: vec![(token.clone(), planned)],
        ..DwNative::default()
    };
    assert!(dw_finish(&mut f, &observed, &desired, "dw-t0", converged).is_none());
    // First Timeout emits the problem.
    dw_advance(&mut f, 100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-t1");
    let e1 = dw_finish(
        &mut f,
        &observed,
        &desired_b,
        "dw-t1",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), DW_STALE)],
            ..DwNative::default()
        },
    )
    .expect("first problem emits");
    assert_eq!(
        dw_rect(dw_entry(&e1, &token).get("post").expect("post")),
        DW_STALE
    );
    // Two drifting repeats inside the throttle window coalesce silently.
    let drift1 = Rect { x: 1, ..DW_STALE };
    dw_advance(&mut f, 100);
    let (_, desired_c) = dw_assemble(&mut f, &observed, "dw-t2");
    assert!(
        dw_finish(
            &mut f,
            &observed,
            &desired_c,
            "dw-t2",
            DwNative {
                written: vec![token.clone()],
                outer: vec![(token.clone(), planned)],
                post: vec![(token.clone(), drift1)],
                ..DwNative::default()
            },
        )
        .is_none(),
        "throttled change stays quiet"
    );
    assert_eq!(f.state.diag_coalesced.len(), 1);
    let drift2 = Rect { x: 2, ..DW_STALE };
    dw_advance(&mut f, 100);
    let (_, desired_d) = dw_assemble(&mut f, &observed, "dw-t3");
    assert!(
        dw_finish(
            &mut f,
            &observed,
            &desired_d,
            "dw-t3",
            DwNative {
                written: vec![token.clone()],
                outer: vec![(token.clone(), planned)],
                post: vec![(token.clone(), drift2)],
                ..DwNative::default()
            },
        )
        .is_none()
    );
    // The same token waiting through a second throttled round counts once:
    // `coalesced` is a unique-token set, never a round counter.
    assert_eq!(f.state.diag_coalesced.len(), 1);
    // Past the window the latest sample delivers with honest counts.
    dw_advance(&mut f, 900);
    let (_, desired_e) = dw_assemble(&mut f, &observed, "dw-t4");
    let e2 = dw_finish(
        &mut f,
        &observed,
        &desired_e,
        "dw-t4",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), drift2)],
            ..DwNative::default()
        },
    )
    .expect("latest delivers past the window");
    assert_eq!(
        dw_rect(dw_entry(&e2, &token).get("post").expect("post")),
        drift2
    );
    assert_eq!(e2.get("coalesced").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(e2.get("omitted").and_then(|v| v.as_u64()), Some(0));
    assert!(f.state.diag_coalesced.is_empty(), "set resets on emit");
    dw_teardown();
}

#[test]
fn alternating_flap_and_recovery_flush() {
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    let wide = dw_hints(Some((1016, 16)));
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let stale = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &stale, "dw-a0");
    let planned = dw_planned(&desired, &token);
    let clamp_native = || DwNative {
        written: vec![token.clone()],
        outer: vec![(token.clone(), planned)],
        post: vec![(token.clone(), DW_STALE)],
        ..DwNative::default()
    };
    let _ = dw_finish(
        &mut f,
        &stale,
        &desired,
        "dw-a0",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    );
    // Timeout problem emits.
    dw_advance(&mut f, 100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_b) = dw_assemble(&mut f, &stale, "dw-a1");
    let e1 = dw_finish(&mut f, &stale, &desired_b, "dw-a1", clamp_native()).expect("problem emits");
    assert_eq!(dw_str(dw_entry(&e1, &token), "fallback_reason"), "reused");
    // Same Hint repeated converges the write: healthy recovery flushes.
    dw_advance(&mut f, 1100);
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let (_, desired_c) = dw_assemble(&mut f, &stale, "dw-a2");
    let e2 = dw_finish(
        &mut f,
        &stale,
        &desired_c,
        "dw-a2",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    )
    .expect("recovery flushes");
    let r2 = dw_entry(&e2, &token);
    assert_eq!(dw_str(r2, "outcome"), "hint");
    assert_eq!(dw_str(r2, "fallback_reason"), "fresh");
    assert_eq!(dw_str(r2, "disposition"), "wrote");
    // Timeout after the renewed base reuses from it and emits again.
    dw_advance(&mut f, 1100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_d) = dw_assemble(&mut f, &stale, "dw-a3");
    let e3 = dw_finish(&mut f, &stale, &desired_d, "dw-a3", clamp_native()).expect("flap re-emits");
    let r3 = dw_entry(&e3, &token);
    assert_eq!(dw_str(r3, "fallback_reason"), "reused");
    assert_eq!(
        r3.get("fallback_age_ms").and_then(|v| v.as_u64()),
        Some(1100)
    );
    // Exact match (window already at target) recovers through the match lane.
    dw_advance(&mut f, 1100);
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let placed = vec![dw_observed(&token, &keys[0], planned)];
    let (_, desired_e) = dw_assemble(&mut f, &placed, "dw-a4");
    assert_eq!(desired_e, desired_d, "settled plan holds");
    let e4 = dw_finish(
        &mut f,
        &placed,
        &desired_e,
        "dw-a4",
        DwNative {
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    )
    .expect("match recovery flushes");
    assert_eq!(dw_str(dw_entry(&e4, &token), "disposition"), "match");
    // Steady repetition emits nothing.
    dw_advance(&mut f, 1100);
    let (_, desired_f) = dw_assemble(&mut f, &placed, "dw-a5");
    assert!(
        dw_finish(
            &mut f,
            &placed,
            &desired_f,
            "dw-a5",
            DwNative {
                post: vec![(token.clone(), planned)],
                ..DwNative::default()
            },
        )
        .is_none(),
        "steady recovery stays quiet"
    );
    dw_teardown();
}

#[test]
fn fallback_reasons_no_hint_identity_invalid() {
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    let wide = dw_hints(Some((1016, 16)));
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-r0");
    let planned = dw_planned(&desired, &token);
    let clamp_native = || DwNative {
        written: vec![token.clone()],
        outer: vec![(token.clone(), planned)],
        post: vec![(token.clone(), DW_STALE)],
        ..DwNative::default()
    };
    let _ = dw_finish(
        &mut f,
        &observed,
        &desired,
        "dw-r0",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    );
    let reason_of =
        |f: &Dw, token: &str| f.state.hint_fallback_report(token).expect("report").reason;
    // Timeout reuses the seed with no time bound.
    dw_advance(&mut f, 100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-r1");
    let e1 =
        dw_finish(&mut f, &observed, &desired_b, "dw-r1", clamp_native()).expect("reuse emits");
    assert_eq!(reason_of(&f, &token), "reused");
    assert_eq!(dw_str(dw_entry(&e1, &token), "fallback_reason"), "reused");
    // A successful query reporting no usable hint clears with reason.
    dw_advance(&mut f, 1100);
    // Empty injected hints read successful-empty with no native call (an
    // uninjected window would fall through to the live query instead).
    dw_fresh_at(&f, &[(keys[0].hwnd, WindowSizeHints::none())]);
    let (_, desired_c) = dw_assemble(&mut f, &observed, "dw-r2");
    let e2 =
        dw_finish(&mut f, &observed, &desired_c, "dw-r2", clamp_native()).expect("no-hint emits");
    assert_eq!(dw_str(dw_entry(&e2, &token), "outcome"), "no-hint");
    assert_eq!(reason_of(&f, &token), "no-hint");
    assert_eq!(dw_str(dw_entry(&e2, &token), "fallback_reason"), "no-hint");
    assert!(
        !f.state.hint_last_good.contains_key(&keys[0]),
        "successful empty clears the entry"
    );
    // A later Timeout finds no entry and cannot revive the previous value.
    dw_advance(&mut f, 1100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_d) = dw_assemble(&mut f, &observed, "dw-r3");
    let e3 =
        dw_finish(&mut f, &observed, &desired_d, "dw-r3", clamp_native()).expect("no-entry emits");
    assert_eq!(reason_of(&f, &token), "no-entry");
    assert_eq!(dw_str(dw_entry(&e3, &token), "fallback_reason"), "no-entry");
    // Fresh stores again and recovers; an unperformed Invalid preserves.
    dw_advance(&mut f, 1100);
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let (_, desired_e) = dw_assemble(&mut f, &observed, "dw-r4");
    let e4 = dw_finish(
        &mut f,
        &observed,
        &desired_e,
        "dw-r4",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    )
    .expect("recovery emits");
    assert_eq!(dw_str(dw_entry(&e4, &token), "outcome"), "hint");
    dw_advance(&mut f, 1100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Invalid);
    let (_, desired_f) = dw_assemble(&mut f, &observed, "dw-r5");
    let e5 =
        dw_finish(&mut f, &observed, &desired_f, "dw-r5", clamp_native()).expect("invalid emits");
    assert_eq!(dw_str(dw_entry(&e5, &token), "outcome"), "invalid");
    assert_eq!(reason_of(&f, &token), "invalid");
    assert_eq!(dw_str(dw_entry(&e5, &token), "fallback_reason"), "invalid");
    assert!(
        f.state.hint_last_good.contains_key(&keys[0]),
        "unperformed query preserves the entry"
    );
    // The preserved entry still serves the next Timeout.
    dw_advance(&mut f, 1100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_g) = dw_assemble(&mut f, &observed, "dw-r6");
    let e6 =
        dw_finish(&mut f, &observed, &desired_g, "dw-r6", clamp_native()).expect("reuse emits");
    assert_eq!(reason_of(&f, &token), "reused");
    assert_eq!(dw_str(dw_entry(&e6, &token), "fallback_reason"), "reused");
    // A live lifetime-tag mismatch (same-process HWND reuse shape) reports
    // identity and drops the suspect entry. The freshly written report
    // survives, but the published class and baseline (bound to the old
    // window) break: the class reads unknown until a trustworthy query
    // re-binds.
    dw_advance(&mut f, 1100);
    inject_pending_tag(keys[0].hwnd, Some("dw-tag-stale".to_owned()));
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_h) = dw_assemble(&mut f, &observed, "dw-r7");
    assert_eq!(reason_of(&f, &token), "identity");
    assert!(
        !f.state.diag_emitted.contains_key(&token),
        "stale baseline breaks on identity mismatch"
    );
    assert_eq!(
        diag_class_cached(&f.state, &token).as_deref(),
        Some(""),
        "recycled HWND reports class unknown"
    );
    let e7 =
        dw_finish(&mut f, &observed, &desired_h, "dw-r7", clamp_native()).expect("identity emits");
    assert_eq!(dw_str(dw_entry(&e7, &token), "fallback_reason"), "identity");
    assert_eq!(dw_str(dw_entry(&e7, &token), "class"), "");
    assert!(
        !f.state.hint_last_good.contains_key(&keys[0]),
        "identity mismatch drops the entry"
    );
    // A performed query under the live tag re-binds: the unknown mark clears
    // without touching the trusted report, and the next emission re-resolves
    // the class live (here a new injection stands in for the new window).
    dw_advance(&mut f, 1100);
    inject_pending_tag(keys[0].hwnd, Some(format!("dw-tag-{token}")));
    inject_diag_class(keys[0].hwnd, "DwClassNew");
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let (_, desired_i) = dw_assemble(&mut f, &observed, "dw-r8");
    assert!(
        diag_class_cached(&f.state, &token).is_none(),
        "trustworthy query clears the unknown mark"
    );
    let e8 = dw_finish(
        &mut f,
        &observed,
        &desired_i,
        "dw-r8",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    )
    .expect("rebind recovery emits");
    assert_eq!(dw_str(dw_entry(&e8, &token), "outcome"), "hint");
    assert_eq!(dw_str(dw_entry(&e8, &token), "class"), "DwClassNew");
    dw_teardown();
}

#[test]
fn no_plan_completes_and_flushes() {
    // Assembly staged a problem row but the Engine reply carries no plan:
    // the sample completes as `no-plan` and still flushes.
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    f.state.tick += 1;
    let mut hint_cx = HintCx::new();
    assemble_domain_rows(
        &mut f.state,
        &f.output,
        &f.ws,
        &observed,
        &[],
        "dw-test",
        "dw-n0",
        &mut hint_cx,
    )
    .expect("rows assemble");
    stage_no_plan(&mut f.state);
    let event = flush_window_diag(&mut f.state).expect("no-plan flushes");
    let entry = dw_entry(&event, &token);
    assert_eq!(dw_str(entry, "outcome"), "timeout");
    assert_eq!(dw_str(entry, "disposition"), "no-plan");
    assert_eq!(dw_str(entry, "fallback_reason"), "no-entry");
    assert_eq!(dw_str(entry, "op"), "dw-test");
    assert_eq!(dw_str(entry, "correlation"), "dw-n0");
    assert!(event.get("op").is_none() && event.get("correlation").is_none());
    dw_teardown();
}

#[test]
fn cap_entries_prune_membership_and_class_cache() {
    const N: usize = 20;
    let mut f = dw_setup(N);
    let keys = dw_keys(&f);
    // Small hints: all twenty members seed well below the domain width, so
    // the Engine plans every member and nothing overconstrains.
    let small = dw_hints(Some((48, 48)));
    dw_fresh_at(
        &f,
        &keys.iter().map(|k| (k.hwnd, small)).collect::<Vec<_>>(),
    );
    let rects: Vec<Rect> = (0..N)
        .map(|i| Rect {
            x: (i as i32) * 96,
            y: 0,
            w: 96,
            h: 108,
        })
        .collect();
    let observed: Vec<ObservedWindow> = keys
        .iter()
        .map(|k| dw_observed(&dw_token(&f, k), k, rects[(k.hwnd - 8200) as usize]))
        .collect();
    // Seed leg stores last-good for every member (no flush: healthy).
    let (_, _) = dw_assemble(&mut f, &observed, "dw-c0");
    // Timeout leg: every member is a changed problem at once.
    for k in &keys {
        inject_test_outcome(k.hwnd, HintOutcome::Timeout);
    }
    dw_advance(&mut f, 100);
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-c1");
    assert_eq!(desired.len(), N, "every member plans");
    let planned_of = |token: &str| dw_planned(&desired, token);
    let native = DwNative {
        written: keys.iter().map(|k| dw_token(&f, k)).collect(),
        outer: keys
            .iter()
            .map(|k| {
                let t = dw_token(&f, k);
                (t.clone(), planned_of(&t))
            })
            .collect(),
        post: observed
            .iter()
            .map(|w| (w.token.clone(), w.visible))
            .collect(),
        ..DwNative::default()
    };
    let e1 = dw_finish(&mut f, &observed, &desired, "dw-c1", native).expect("capped batch emits");
    let list1 = e1
        .get("entries")
        .expect("entries")
        .as_array()
        .expect("array");
    assert_eq!(list1.len(), 16, "entries cap at sixteen");
    assert_eq!(e1.get("omitted").and_then(|v| v.as_u64()), Some(4));
    let windows1: Vec<&str> = list1
        .iter()
        .map(|e| e.get("window").and_then(|w| w.as_str()).expect("window"))
        .collect();
    let mut sorted = windows1.clone();
    sorted.sort();
    assert_eq!(windows1, sorted, "entries sort by token");
    // Past the window the remainder delivers.
    dw_advance(&mut f, 1100);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-c2");
    let native_b = DwNative {
        written: keys.iter().map(|k| dw_token(&f, k)).collect(),
        outer: keys
            .iter()
            .map(|k| {
                let t = dw_token(&f, k);
                (t.clone(), dw_planned(&desired_b, &t))
            })
            .collect(),
        post: observed
            .iter()
            .map(|w| (w.token.clone(), w.visible))
            .collect(),
        ..DwNative::default()
    };
    let e2 = dw_finish(&mut f, &observed, &desired_b, "dw-c2", native_b).expect("remainder emits");
    assert_eq!(
        e2.get("entries")
            .expect("entries")
            .as_array()
            .expect("array")
            .len(),
        4
    );
    assert_eq!(e2.get("omitted").and_then(|v| v.as_u64()), Some(0));
    // Emission resolved classes once; membership prune drops everything.
    assert_eq!(
        diag_class_cached(&f.state, "dw01").as_deref(),
        Some("DwClass0")
    );
    let drop_key = keys[16].clone();
    let drop_token = dw_token(&f, &drop_key);
    drop_member_state(&mut f.state, &drop_key);
    assert!(
        !f.state.diag_pending.contains_key(&drop_token),
        "pending pruned with membership"
    );
    assert!(
        !f.state.diag_emitted.contains_key(&drop_token),
        "emitted pruned with membership"
    );
    assert!(
        diag_class_cached(&f.state, &drop_token).is_none(),
        "class pruned with membership"
    );
    // Hours later the survivors still reuse with no time bound. The post
    // frames drift by a pixel so every survivor deterministically differs
    // from its baseline (reflow alone might hold some rectangles); the batch
    // honestly re-emits (capped) with reuse reasons throughout and no
    // residue of the dropped member.
    dw_advance(&mut f, 3 * 3600 * 1000);
    let survivors: Vec<ObservedWindow> = observed
        .iter()
        .filter(|w| w.token != drop_token)
        .cloned()
        .collect();
    let (rows_c, desired_c) = dw_assemble(&mut f, &survivors, "dw-c3");
    assert_eq!(desired_c.len(), N - 1, "every survivor still plans");
    assert!(
        rows_c.iter().all(|row| !row.hints.is_empty()),
        "hours-old entries still serve"
    );
    let native_c = DwNative {
        written: survivors.iter().map(|w| w.token.clone()).collect(),
        outer: survivors
            .iter()
            .map(|w| (w.token.clone(), dw_planned(&desired_c, &w.token)))
            .collect(),
        post: survivors
            .iter()
            .map(|w| {
                (
                    w.token.clone(),
                    Rect {
                        x: w.visible.x + 5,
                        ..w.visible
                    },
                )
            })
            .collect(),
        ..DwNative::default()
    };
    let e3 =
        dw_finish(&mut f, &survivors, &desired_c, "dw-c3", native_c).expect("reflow batch emits");
    let list3 = e3
        .get("entries")
        .expect("entries")
        .as_array()
        .expect("array");
    assert_eq!(list3.len(), 16);
    assert_eq!(e3.get("omitted").and_then(|v| v.as_u64()), Some(3));
    assert!(
        list3
            .iter()
            .all(|e| e.get("window").and_then(|w| w.as_str()) != Some(drop_token.as_str())),
        "dropped member leaves no residue"
    );
    assert!(
        list3
            .iter()
            .all(|e| e.get("fallback_reason").and_then(|r| r.as_str()) == Some("reused")),
        "every survivor still reuses with no time bound"
    );
    dw_teardown();
}

#[test]
fn suppression_and_rearm_reasons() {
    // Five members, one batch: each exercises one refused-lane reason
    // through the production derivation over real tracker states.
    let mut f = dw_setup(5);
    let keys = dw_keys(&f);
    // Small hints: every member plans well above its floor, so no entry
    // overconstrains and the scripted pairs below compare against the
    // staged effective rectangles exactly.
    let small = dw_hints(Some((48, 48)));
    dw_fresh_at(
        &f,
        &keys.iter().map(|k| (k.hwnd, small)).collect::<Vec<_>>(),
    );
    let toks: Vec<String> = keys.iter().map(|k| dw_token(&f, k)).collect();
    // Staging reads the pre-write frame from the observation itself, so the
    // scripted lane pairs below bind to these observed rectangles (each
    // deliberately off-plan, the app-held shape).
    let obs = |i: usize| Rect {
        x: 500 + i as i32 * 10,
        y: 0,
        w: 200,
        h: 200,
    };
    let observed: Vec<ObservedWindow> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| dw_observed(&toks[i], k, obs(i)))
        .collect();
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-s0");
    let eff = |i: usize| dw_planned(&desired, &toks[i]);
    assert_ne!(eff(0), obs(0), "clamp needs a mismatch");
    // Staging reads the pre-write frame from the observation itself, so the
    // scripted lane pairs below bind to these observed rectangles (each
    // deliberately off-plan, the app-held shape).
    let obs = |i: usize| Rect {
        x: 500 + i as i32 * 10,
        y: 0,
        w: 200,
        h: 200,
    };
    // Pre-write snapshot mirroring production apply entry: clamp lanes for
    // dw01 (identical), dw03 (old desired), dw04 (old observed); transient
    // lanes for dw02 and dw05.
    let mut snapshot = crate::tiling::RefusedTracker::default();
    snapshot.note_clamp(&toks[0].clone(), &eff(0), &obs(0));
    snapshot.note_transient(&toks[1].clone(), &eff(1), &obs(1), f.now);
    snapshot.note_clamp(
        &toks[2].clone(),
        &Rect {
            x: eff(2).x + 40,
            ..eff(2)
        },
        &obs(2),
    );
    snapshot.note_clamp(
        &toks[3].clone(),
        &eff(3),
        &Rect {
            x: obs(3).x + 40,
            ..obs(3)
        },
    );
    snapshot.note_transient(&toks[4].clone(), &eff(4), &obs(4), f.now);
    let native = DwNative {
        written: vec![toks[2].clone(), toks[3].clone(), toks[4].clone()],
        skipped: vec![
            (toks[0].clone(), "refused-intent".to_owned()),
            (toks[1].clone(), "refused-intent".to_owned()),
        ],
        outer: vec![
            (toks[2].clone(), eff(2)),
            (toks[3].clone(), eff(3)),
            (toks[4].clone(), eff(4)),
        ],
        post: vec![
            (toks[0].clone(), obs(0)),
            (toks[1].clone(), obs(1)),
            (toks[2].clone(), obs(2)),
            (toks[3].clone(), obs(3)),
            (toks[4].clone(), obs(4)),
        ],
    };
    let event = dw_finish_with(&mut f, &observed, &desired, "dw-s0", native, &snapshot)
        .expect("suppression batch emits");
    let entries = event
        .get("entries")
        .expect("entries")
        .as_array()
        .expect("array");
    assert_eq!(entries.len(), 5);
    let check = |i: usize, disposition: &str, suppress: &str| {
        let entry = dw_entry(&event, &toks[i]);
        assert_eq!(dw_str(entry, "disposition"), disposition, "dw{:02}", i + 1);
        assert_eq!(dw_str(entry, "suppress"), suppress, "dw{:02}", i + 1);
    };
    check(0, "refused-intent", "identical-clamp");
    check(1, "refused-intent", "transient-backoff");
    check(2, "wrote", "desired-changed");
    check(3, "wrote", "observed-changed");
    check(4, "wrote", "transient-expired");
    dw_teardown();
}

#[test]
fn exact_match_clears_transient_lane() {
    // A held transient pair that the post-write readback converges reports
    // `exact-match` on the recovery flush: the identical pair plus a
    // matching post is the lane clearing, not a new suppression.
    // (Equal clamp pairs can never reach the derivation: `note_clamp`
    // clears those at record time, so only the transient lane qualifies.)
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    let wide = dw_hints(Some((1016, 16)));
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let stale = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &stale, "dw-m0");
    let planned = dw_planned(&desired, &token);
    let mut snapshot = crate::tiling::RefusedTracker::default();
    snapshot.note_transient(&token, &planned, &DW_STALE, f.now);
    let e1 = dw_finish_with(
        &mut f,
        &stale,
        &desired,
        "dw-m0",
        DwNative {
            skipped: vec![(token.clone(), "refused-intent".to_owned())],
            post: vec![(token.clone(), DW_STALE)],
            ..DwNative::default()
        },
        &snapshot,
    )
    .expect("transient baseline emits");
    assert_eq!(
        dw_str(dw_entry(&e1, &token), "suppress"),
        "transient-backoff"
    );
    // Past the backoff the setter runs and the readback converges: the same
    // held pair now reads as the lane clearing.
    dw_advance(&mut f, 1100);
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let (_, desired_b) = dw_assemble(&mut f, &stale, "dw-m1");
    assert_eq!(desired_b, desired, "settled plan holds");
    let mut snapshot_b = crate::tiling::RefusedTracker::default();
    snapshot_b.note_transient(&token, &planned, &DW_STALE, f.t0);
    let e2 = dw_finish_with(
        &mut f,
        &stale,
        &desired_b,
        "dw-m1",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
        &snapshot_b,
    )
    .expect("converged recovery flushes");
    let r2 = dw_entry(&e2, &token);
    assert_eq!(dw_str(r2, "disposition"), "wrote");
    assert_eq!(dw_str(r2, "suppress"), "exact-match");
    dw_advance(&mut f, 1100);
    let (_, desired_c) = dw_assemble(&mut f, &stale, "dw-m2");
    assert!(
        dw_finish(
            &mut f,
            &stale,
            &desired_c,
            "dw-m2",
            DwNative {
                written: vec![token.clone()],
                outer: vec![(token.clone(), planned)],
                post: vec![(token.clone(), planned)],
                ..DwNative::default()
            },
        )
        .is_none(),
        "cleared lane stays quiet"
    );
    dw_teardown();
}

#[test]
fn actual_query_cost_and_prequery_skip() {
    // The staged cost is the ACTUAL native cost, not the pre-read estimate:
    // a real (failing) native query on an uninjected window reports the
    // offered timeout recomputed at the call plus a measured round-trip,
    // while prework refusals report zero without any native call.
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    // Visible lane with no injection: the native query runs and fails fast
    // on the synthetic HWND (Failed), recording the true offered bound.
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-q0");
    let report = f
        .state
        .hint_fallback_report(&token)
        .expect("fallback report");
    assert_eq!(report.reason, "failed");
    let planned = dw_planned(&desired, &token);
    let placed: Vec<ObservedWindow> = vec![dw_observed(&token, &keys[0], planned)];
    // Re-stage the write lane against the converged frame so the failure
    // sample (not geometry) is the whole story.
    let e0 = dw_finish(
        &mut f,
        &placed,
        &desired,
        "dw-q0",
        DwNative {
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    )
    .expect("failed query emits");
    let entry = dw_entry(&e0, &token);
    assert_eq!(dw_str(entry, "outcome"), "failed");
    // The offered bound is nonzero for an attempted query (an attempted
    // query with zero remaining budget would report Budget instead); the
    // exact bound jitters with remaining budget and stays advisory.
    let offered = entry
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .expect("timeout");
    assert!(
        (1..=10).contains(&offered),
        "actual offered bound at the native call"
    );
    assert!(
        entry.get("query_us").and_then(|v| v.as_u64()).is_some(),
        "measured native round-trip rides the entry"
    );
    // Hidden lane with no member state: Invalid before any OS handle use,
    // cost honestly zero.
    let missing = crate::workspace::WindowKey {
        hwnd: 99999,
        pid: 1,
        creation: "cdw-missing".to_owned(),
    };
    let cx = HintCx::new();
    let fresh = hidden_hint_for(&f.state, &missing, &cx);
    assert_eq!(fresh.outcome, HintOutcome::Invalid);
    assert_eq!(fresh.cost.offered_ms, 0);
    assert_eq!(fresh.cost.elapsed_us, 0);
    dw_teardown();
}

#[test]
fn token_remint_reports_no_entry_and_prunes_old_identity() {
    // Same key, reminted token: the stored lifecycle no longer binds, the
    // entry drops without revival, and the old token's class and baseline
    // prune with membership.
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    let wide = dw_hints(Some((1016, 16)));
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-tm0");
    let planned = dw_planned(&desired, &token);
    let _ = dw_finish(
        &mut f,
        &observed,
        &desired,
        "dw-tm0",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    );
    // Timeout reuse emits under the original token (populating its class).
    dw_advance(&mut f, 100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-tm1");
    let _ = dw_finish(
        &mut f,
        &observed,
        &desired_b,
        "dw-tm1",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), DW_STALE)],
            ..DwNative::default()
        },
    )
    .expect("reuse emits");
    assert!(diag_class_cached(&f.state, &token).is_some());
    // Remint: same member key, fresh token string and tag.
    let new_token = "dw01b".to_owned();
    f.state
        .member_tokens
        .insert(keys[0].clone(), new_token.clone());
    f.state
        .member_tags
        .insert(keys[0].clone(), "dw-tag-dw01b".to_owned());
    inject_pending_tag(keys[0].hwnd, Some("dw-tag-dw01b".to_owned()));
    let observed_b = vec![dw_observed(&new_token, &keys[0], DW_STALE)];
    dw_advance(&mut f, 1100);
    let (_, desired_c) = dw_assemble(&mut f, &observed_b, "dw-tm2");
    assert_eq!(
        f.state
            .hint_fallback_report(&new_token)
            .expect("report")
            .reason,
        "no-entry"
    );
    assert!(
        !f.state.hint_last_good.contains_key(&keys[0]),
        "moved lifecycle drops the entry"
    );
    assert!(
        !f.state.diag_pending.contains_key(&token),
        "old token prunes with membership"
    );
    assert!(
        !f.state.diag_emitted.contains_key(&token),
        "old baseline prunes with membership"
    );
    assert!(
        diag_class_cached(&f.state, &token).is_none(),
        "old class prunes with membership"
    );
    let e3 = dw_finish(
        &mut f,
        &observed_b,
        &desired_c,
        "dw-tm2",
        DwNative {
            written: vec![new_token.clone()],
            outer: vec![(new_token.clone(), dw_planned(&desired_c, &new_token))],
            post: vec![(new_token.clone(), DW_STALE)],
            ..DwNative::default()
        },
    )
    .expect("reminted problem emits under the new token");
    assert_eq!(
        dw_str(dw_entry(&e3, &new_token), "fallback_reason"),
        "no-entry"
    );
    dw_teardown();
}

#[test]
fn fair_rotation_eventually_emits_every_member_including_recovery() {
    // Twenty continuously changing members over four intervals: emission
    // rotates by per-token emission order (never-emitted, then oldest), so
    // the first-sixteen head can never starve the tail, and a mid-run
    // recovery still delivers.
    const N: usize = 20;
    let mut f = dw_setup(N);
    let keys = dw_keys(&f);
    let small = dw_hints(Some((48, 48)));
    dw_fresh_at(
        &f,
        &keys.iter().map(|k| (k.hwnd, small)).collect::<Vec<_>>(),
    );
    let observed: Vec<ObservedWindow> = keys
        .iter()
        .map(|k| {
            let i = (k.hwnd - 8200) as i32;
            dw_observed(
                &dw_token(&f, k),
                k,
                Rect {
                    x: i * 96,
                    y: 0,
                    w: 96,
                    h: 108,
                },
            )
        })
        .collect();
    let (_, seed_desired) = dw_assemble(&mut f, &observed, "dw-f0");
    assert_eq!(seed_desired.len(), N, "every member plans");
    for k in &keys {
        inject_test_outcome(k.hwnd, HintOutcome::Timeout);
    }
    let mut seen: HashSet<String> = HashSet::new();
    for round in 1..=3u64 {
        dw_advance(&mut f, if round == 1 { 100 } else { 1100 });
        let corr = format!("dw-f{round}");
        let (_, desired) = dw_assemble(&mut f, &observed, &corr);
        // Monotonic post churn: every member differs from any older
        // baseline, so all twenty contend every round.
        let native = DwNative {
            written: keys.iter().map(|k| dw_token(&f, k)).collect(),
            outer: keys
                .iter()
                .map(|k| {
                    let t = dw_token(&f, k);
                    (t.clone(), dw_planned(&desired, &t))
                })
                .collect(),
            post: keys
                .iter()
                .map(|k| {
                    (
                        dw_token(&f, k),
                        Rect {
                            x: round as i32 * 3,
                            y: 0,
                            w: 96,
                            h: 108,
                        },
                    )
                })
                .collect(),
            ..DwNative::default()
        };
        let event =
            dw_finish(&mut f, &observed, &desired, &corr, native).expect("contended rounds emit");
        let list = event
            .get("entries")
            .expect("entries")
            .as_array()
            .expect("array");
        assert_eq!(list.len(), 16, "round {round} caps at sixteen");
        assert_eq!(event.get("omitted").and_then(|v| v.as_u64()), Some(4));
        seen.extend(list.iter().map(|e| {
            e.get("window")
                .and_then(|w| w.as_str())
                .expect("window")
                .to_owned()
        }));
    }
    let all: HashSet<String> = keys.iter().map(|k| dw_token(&f, k)).collect();
    assert_eq!(seen, all, "every member emits within three intervals");
    // Round four: dw20 recovers while the rest keep churning. Its recovery
    // is among the oldest pending changes and must deliver despite the
    // crowd.
    dw_fresh_at(
        &f,
        &keys.iter().map(|k| (k.hwnd, small)).collect::<Vec<_>>(),
    );
    for k in &keys[..N - 1] {
        inject_test_outcome(k.hwnd, HintOutcome::Timeout);
    }
    dw_advance(&mut f, 1100);
    let (_, desired_d) = dw_assemble(&mut f, &observed, "dw-f4");
    let planned20 = dw_planned(&desired_d, "dw20");
    let native_d = DwNative {
        written: keys.iter().map(|k| dw_token(&f, k)).collect(),
        outer: keys
            .iter()
            .map(|k| {
                let t = dw_token(&f, k);
                (t.clone(), dw_planned(&desired_d, &t))
            })
            .collect(),
        post: keys
            .iter()
            .map(|k| {
                let t = dw_token(&f, k);
                let post = if t == "dw20" {
                    planned20
                } else {
                    Rect {
                        x: 12,
                        y: 0,
                        w: 96,
                        h: 108,
                    }
                };
                (t, post)
            })
            .collect(),
        ..DwNative::default()
    };
    let e4 = dw_finish(&mut f, &observed, &desired_d, "dw-f4", native_d).expect("round four emits");
    let recovery = dw_entry(&e4, "dw20");
    assert_eq!(dw_str(recovery, "outcome"), "hint");
    assert_eq!(dw_str(recovery, "disposition"), "wrote");
    assert_eq!(
        dw_rect(recovery.get("post").expect("post")),
        planned20,
        "recovery carries the converged frame"
    );
    dw_teardown();
}

#[test]
fn stale_completed_sample_flushes_with_original_context() {
    // A completed sample stays eligible across idle ticks: a throttled
    // recovery emits later in its ORIGINAL staging context (operation,
    // correlation, tick), never relabeled to the flush context, while an
    // unchanged stale sample stays quiet without being dropped.
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    let wide = dw_hints(Some((1016, 16)));
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-g0");
    let planned = dw_planned(&desired, &token);
    let converged = || DwNative {
        written: vec![token.clone()],
        outer: vec![(token.clone(), planned)],
        post: vec![(token.clone(), planned)],
        ..DwNative::default()
    };
    assert!(dw_finish(&mut f, &observed, &desired, "dw-g0", converged()).is_none());
    // Problem emits under the first correlation.
    dw_advance(&mut f, 100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-g1");
    let e1 = dw_finish(
        &mut f,
        &observed,
        &desired_b,
        "dw-g1",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), DW_STALE)],
            ..DwNative::default()
        },
    )
    .expect("problem emits");
    assert_eq!(dw_str(dw_entry(&e1, &token), "correlation"), "dw-g1");
    // Healthy recovery staged one tick later throttles behind the problem.
    dw_advance(&mut f, 100);
    dw_fresh_at(&f, &[(keys[0].hwnd, wide)]);
    let staging_tick = f.state.tick + 1;
    let (_, desired_c) = dw_assemble(&mut f, &observed, "dw-g2");
    assert_eq!(f.state.tick, staging_tick);
    assert!(
        dw_finish(&mut f, &observed, &desired_c, "dw-g2", converged()).is_none(),
        "recovery waits through the throttle"
    );
    assert_eq!(f.state.diag_coalesced.len(), 1);
    // An idle tick (no staging at all) then a later flush: the recovery
    // delivers with its staging context intact.
    dw_advance(&mut f, 1100);
    f.state.tick += 1;
    let idle_tick = f.state.tick;
    assert!(idle_tick > staging_tick);
    let e2 = flush_window_diag(&mut f.state).expect("stale recovery flushes");
    let recovery = dw_entry(&e2, &token);
    assert_eq!(dw_str(recovery, "outcome"), "hint");
    assert_eq!(dw_str(recovery, "op"), "dw-test");
    assert_eq!(dw_str(recovery, "correlation"), "dw-g2");
    assert_eq!(
        recovery.get("tick").and_then(|v| v.as_u64()),
        Some(staging_tick),
        "entry keeps its staging tick, not the flush tick"
    );
    assert_eq!(e2.get("tick").and_then(|v| v.as_u64()), Some(idle_tick));
    assert!(e2.get("op").is_none() && e2.get("correlation").is_none());
    assert_eq!(e2.get("coalesced").and_then(|v| v.as_u64()), Some(1));
    // One more idle tick: the now-current baseline stays quiet.
    f.state.tick += 1;
    assert!(
        flush_window_diag(&mut f.state).is_none(),
        "unchanged stale sample stays quiet"
    );
    dw_teardown();
}

#[test]
fn problem_steady_repetition_stays_quiet() {
    // The same problem restaged past the throttle window emits nothing new.
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    dw_fresh_at(&f, &[(keys[0].hwnd, dw_hints(Some((1016, 16))))]);
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-q1");
    let planned = dw_planned(&desired, &token);
    let clamped = || DwNative {
        written: vec![token.clone()],
        outer: vec![(token.clone(), planned)],
        post: vec![(token.clone(), DW_STALE)],
        ..DwNative::default()
    };
    let _ = dw_finish(
        &mut f,
        &observed,
        &desired,
        "dw-q1",
        DwNative {
            written: vec![token.clone()],
            outer: vec![(token.clone(), planned)],
            post: vec![(token.clone(), planned)],
            ..DwNative::default()
        },
    );
    dw_advance(&mut f, 100);
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-q2");
    assert!(
        dw_finish(&mut f, &observed, &desired_b, "dw-q2", clamped()).is_some(),
        "first problem emits"
    );
    dw_advance(&mut f, 1100);
    let (_, desired_c) = dw_assemble(&mut f, &observed, "dw-q3");
    assert!(
        dw_finish(&mut f, &observed, &desired_c, "dw-q3", clamped()).is_none(),
        "identical problem repetition stays quiet"
    );
    dw_teardown();
}

#[test]
fn emission_class_fence_reports_unknown_on_lifetime_mismatch() {
    // Emission-time fence: no cached class and a Timeout with no last-good
    // entry (so no identity-break mark either), but the live lifetime
    // already moved. The entry still emits (Timeout is a problem) with
    // unknown class, and the fence failure caches nothing so a later tick
    // retries. Same for a pid mismatch; restoring both re-resolves live.
    let mut f = dw_setup(1);
    let keys = dw_keys(&f);
    let token = dw_token(&f, &keys[0]);
    let pid = keys[0].pid;
    inject_test_outcome(keys[0].hwnd, HintOutcome::Timeout);
    inject_pending_tag(keys[0].hwnd, Some("dw-tag-stale".to_owned()));
    let observed = vec![dw_observed(&token, &keys[0], DW_STALE)];
    let (_, desired) = dw_assemble(&mut f, &observed, "dw-f0");
    let planned = dw_planned(&desired, &token);
    let mismatch = |post: Rect| DwNative {
        written: vec![token.clone()],
        outer: vec![(token.clone(), planned)],
        post: vec![(token.clone(), post)],
        ..DwNative::default()
    };
    let e0 = dw_finish(&mut f, &observed, &desired, "dw-f0", mismatch(DW_STALE))
        .expect("timeout problem emits");
    assert_eq!(
        dw_str(dw_entry(&e0, &token), "class"),
        "",
        "stale tag reports unknown class"
    );
    assert!(
        diag_class_cached(&f.state, &token).is_none(),
        "fence failure caches nothing"
    );
    // Pid mismatch leg: tag restored, live pid wrong.
    dw_advance(&mut f, 1100);
    inject_pending_tag(keys[0].hwnd, Some(format!("dw-tag-{token}")));
    inject_diag_pid(keys[0].hwnd, pid + 1);
    let (_, desired_b) = dw_assemble(&mut f, &observed, "dw-f1");
    let drift1 = Rect { x: 1, ..DW_STALE };
    let e1 = dw_finish(&mut f, &observed, &desired_b, "dw-f1", mismatch(drift1))
        .expect("pid mismatch emits");
    assert_eq!(dw_str(dw_entry(&e1, &token), "class"), "");
    assert!(
        diag_class_cached(&f.state, &token).is_none(),
        "pid fence failure caches nothing"
    );
    // Restored lifetime re-resolves live and caches.
    dw_advance(&mut f, 1100);
    inject_diag_pid(keys[0].hwnd, pid);
    let (_, desired_c) = dw_assemble(&mut f, &observed, "dw-f2");
    let drift2 = Rect { x: 2, ..DW_STALE };
    let e2 = dw_finish(&mut f, &observed, &desired_c, "dw-f2", mismatch(drift2))
        .expect("restored lifetime emits");
    assert_eq!(dw_str(dw_entry(&e2, &token), "class"), "DwClass0");
    assert_eq!(
        diag_class_cached(&f.state, &token).as_deref(),
        Some("DwClass0")
    );
    dw_teardown();
}
