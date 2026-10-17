#![forbid(unsafe_code)]
// Windows-only last-good declared-hint reuse on native query Timeout.
// LOG-DERIVED OUTCOMES with synthetic geometry (never a pixel-exact replay):
// the incident record shows Hint -> Timeout -> (quiet repeats) -> same Hint
// repeated, with four-member ticks rewriting all four members while the hint
// is unknown (tick 127: unknown writes 4 mismatch 1; tick 128: unknown
// applied 0 mismatch 1; tick 129: unknown applied 4 mismatch 1) and rewriting
// all four back to match on hint recovery (tick 130: hint writes 4 match).
// Exact incident rects/minimums are absent, so every rectangle/minimum below
// is synthetic and only the outcome shape (stable targets through a Timeout,
// reflow on genuine loss) is asserted.
use super::rmax03_adapter_tests::test_state;
use std::time::{Duration, Instant};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;
use tiler_core::size_hints::WindowSizeHints;

/// One synthetic visible member with distinct fake identity per token. The
/// cfg(test) query seam keys injections by HWND, so members never share one.
fn tf_member(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    token: &str,
    hwnd: u64,
) -> crate::workspace::WindowKey {
    let key = crate::workspace::WindowKey {
        hwnd,
        pid: 7000 + hwnd as u32,
        creation: format!("ctf{hwnd:016x}"),
    };
    assert!(state.workspaces.assign(key.clone(), output, ws, false));
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-tf".to_owned(),
            session_id: 1,
            exe_path: "tf.exe".to_owned(),
        },
    );
    state
        .member_tags
        .insert(key.clone(), format!("tf-tag-{token}"));
    state.last_hwnds.insert(key.hwnd);
    super::inject_pending_tag(key.hwnd, Some(format!("tf-tag-{token}")));
    key
}

fn tf_domain(
    output: &str,
    ws: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    crate::workspace_owner::workspace_domain(output, ws, bounds, 8)
}

/// Synthetic visible observation for one member (synthetic geometry only).
fn tf_observed(
    token: &str,
    key: &crate::workspace::WindowKey,
    rect: Rect,
) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: "tf.exe".to_owned(),
            user_sid: "S-1-5-tf".to_owned(),
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

/// Assemble one domain through production assembly and commit it through the
/// production stamped handle. Returns the committed rows plus the Engine
/// reply.
fn tf_assemble_and_commit(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    observed: &[super::ObservedWindow],
    focus: &str,
    correlation: &str,
) -> (
    Vec<crate::workspace_owner::OwnerRow>,
    tiler_core::boundary::CoreReply,
) {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    for window in observed {
        state.overlay_now.insert(
            window.token.clone(),
            (window.facts.maximized, window.facts.captionless_fullscreen),
        );
    }
    let mut hint_cx = super::HintCx::new();
    let rows = super::assemble_domain_rows(
        state,
        output,
        ws,
        observed,
        &[],
        "tf-test",
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
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse(correlation).expect("correlation"),
        super::revision_for(state, output, ws),
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focus.to_owned())),
    );
    event.command = CoreCommand::Reconcile;
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        matches!(reply, CoreReply::Tiled(_) | CoreReply::Projection(_)),
        "commit converges, got {reply:?}"
    );
    (rows, reply)
}

/// Planned Engine geometry per token, when the reply carries a plan.
fn tf_planned(reply: &tiler_core::boundary::CoreReply) -> Vec<(String, Rect)> {
    let mut planned: Vec<(String, Rect)> = crate::workspace_owner::planned_writes(reply)
        .unwrap_or_default()
        .into_iter()
        .map(|w| (w.window.0.clone(), w.rect))
        .collect();
    planned.sort_by(|a, b| a.0.cmp(&b.0));
    planned
}

/// Synthetic converted hints through the production conversion path (never
/// hand-built): outer track minus 8px frames on each axis.
fn tf_hints(min_outer: Option<(i32, i32)>) -> WindowSizeHints {
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

fn tf_quarters() -> [Rect; 4] {
    [
        Rect {
            x: 0,
            y: 0,
            w: 480,
            h: 1080,
        },
        Rect {
            x: 480,
            y: 0,
            w: 480,
            h: 1080,
        },
        Rect {
            x: 960,
            y: 0,
            w: 480,
            h: 1080,
        },
        Rect {
            x: 1440,
            y: 0,
            w: 480,
            h: 1080,
        },
    ]
}

struct TfFour {
    state: super::TileLoop,
    output: String,
    ws: String,
    domain: (
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    keys: Vec<crate::workspace::WindowKey>,
    t0: Instant,
}

/// Four synthetic visible members with pinned clock and scripted lifetime
/// tags. Hint injections are per-tick (never shared): tests inject exactly
/// the outcomes each leg needs.
fn tf_setup() -> TfFour {
    super::clear_test_hints();
    super::clear_pending_seams();
    let t0 = Instant::now();
    super::inject_hint_now(Some(t0));
    let mut state = test_state();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let domain = tf_domain(&output, &ws);
    let tokens = ["w1", "w2", "w3", "w4"];
    let mut keys = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        keys.push(tf_member(&mut state, &output, &ws, token, 8100 + i as u64));
    }
    TfFour {
        state,
        output,
        ws,
        domain,
        keys,
        t0,
    }
}

fn tf_observed_all(f: &TfFour) -> Vec<super::ObservedWindow> {
    let tokens = ["w1", "w2", "w3", "w4"];
    let quarters = tf_quarters();
    tokens
        .iter()
        .zip(f.keys.iter())
        .zip(quarters.iter())
        .map(|((token, key), rect)| tf_observed(token, key, *rect))
        .collect()
}

/// Inject a fresh-hint tick: usable hints for `hinted` HWNDs, unknown for the
/// rest (empty hints read Invalid with no native call).
fn tf_inject_fresh(f: &TfFour, hints: &[(u64, WindowSizeHints)]) {
    for key in &f.keys {
        match hints.iter().find(|(hwnd, _)| *hwnd == key.hwnd) {
            Some((_, h)) => super::inject_test_hints(key.hwnd, *h),
            None => super::inject_test_hints(key.hwnd, WindowSizeHints::none()),
        }
    }
}

fn tf_tick(
    f: &mut TfFour,
    observed: &[super::ObservedWindow],
    correlation: &str,
) -> (
    Vec<crate::workspace_owner::OwnerRow>,
    tiler_core::boundary::CoreReply,
) {
    f.state.tick += 1;
    let output = f.output.clone();
    let ws = f.ws.clone();
    tf_assemble_and_commit(
        &mut f.state,
        &output,
        &ws,
        &f.domain,
        observed,
        "w4",
        correlation,
    )
}

fn tf_row_hints(rows: &[crate::workspace_owner::OwnerRow], token: &str) -> WindowSizeHints {
    rows.iter()
        .find(|row| row.token == token)
        .expect("row")
        .hints
}

/// Clear hint/outcome seams and re-pin the clock, then inject one fresh tick.
fn tf_fresh_at(f: &TfFour, at: Instant, hints: &[(u64, WindowSizeHints)]) {
    super::clear_test_hints();
    super::inject_hint_now(Some(at));
    tf_inject_fresh(f, hints);
}

fn tf_teardown() {
    super::clear_test_hints();
    super::clear_pending_seams();
}

#[test]
fn timeout_reuses_last_good_and_keeps_sibling_allocation() {
    // Log-derived sequence shape with synthetic geometry (never pixel-exact):
    // Hint -> Timeout -> Timeout -> Timeout (quiet repeats) -> same Hint
    // repeated -> Timeout -> same Hint. Planned targets must not move on any
    // leg; only a genuine fresh change reallocates siblings. The
    // successful-but-clamped tracker pair below rides the real
    // RefusedTracker through the same legs with no production tracker
    // change: the identical pair stays suppressed across fallback, a genuine
    // change re-arms, and a matching readback clears.
    let mut f = tf_setup();
    // Synthetic minimum: outer 1016x16 minus 8px frames = visible 1000 wide
    // with no usable height bound. The 1000px floor exceeds the 480px fair
    // share, so genuine loss reflows the siblings.
    let wide = tf_hints(Some((1016, 16)));
    assert_eq!(wide.min_w, Some(1000));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let (rows_a, reply_a) = tf_tick(&mut f, &observed, "tf-a");
    assert_eq!(tf_row_hints(&rows_a, "w1"), wide);
    let plan_a = tf_planned(&reply_a);
    assert!(!plan_a.is_empty(), "seed tick plans geometry");
    // Settle on the identical observation: planned targets already stable.
    let (_, reply_a2) = tf_tick(&mut f, &observed, "tf-a2");
    assert_eq!(tf_planned(&reply_a2), plan_a, "settled tick holds targets");
    // Successful-but-clamped pair: the write succeeded while the readback
    // still shows the synthetic pre-plan frame, so the identical pair
    // suppresses until either side genuinely changes.
    let desired_w1 = plan_a
        .iter()
        .find(|(token, _)| token == "w1")
        .expect("w1 target")
        .1;
    let readback_w1 = tf_quarters()[0];
    assert_ne!(desired_w1, readback_w1, "synthetic clamp needs a mismatch");
    let mut refused = super::RefusedTracker::default();
    refused.note_clamp("w1", &desired_w1, &readback_w1);
    // Quiet Timeout repeats on a controlled increasing clock.
    for (leg, ms) in [100u64, 600, 1200].iter().enumerate() {
        let at = f.t0 + Duration::from_millis(*ms);
        super::inject_hint_now(Some(at));
        super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
        let corr = format!("tf-b{leg}");
        let (rows, reply) = tf_tick(&mut f, &observed, &corr);
        assert_eq!(tf_row_hints(&rows, "w1"), wide, "Timeout reuses hint");
        let report = f.state.hint_fallback_report("w1").expect("report");
        assert!(report.reused, "Timeout leg reports reuse");
        assert_eq!(report.age_ms, *ms, "reuse age tracks the last success");
        assert_eq!(tf_planned(&reply), plan_a, "Timeout holds targets");
        assert!(
            refused.should_skip("w1", &desired_w1, &readback_w1, at),
            "identical clamp pair stays suppressed across fallback"
        );
    }
    // Same Hint repeated: fresh evidence, no reuse report, same targets.
    tf_fresh_at(
        &f,
        f.t0 + Duration::from_millis(1300),
        &[(f.keys[0].hwnd, wide)],
    );
    let (rows_c, reply_c) = tf_tick(&mut f, &observed, "tf-c");
    assert_eq!(tf_row_hints(&rows_c, "w1"), wide);
    assert!(
        !f.state.hint_fallback_report("w1").expect("report").reused,
        "fresh rows never report reuse"
    );
    assert_eq!(tf_planned(&reply_c), plan_a);
    // Timeout after the repeated Hint reuses from the renewed base (+1300):
    // age 200, measured from the last successful query.
    super::inject_hint_now(Some(f.t0 + Duration::from_millis(1500)));
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let (rows_d, reply_d) = tf_tick(&mut f, &observed, "tf-d");
    assert_eq!(tf_row_hints(&rows_d, "w1"), wide);
    let report = f.state.hint_fallback_report("w1").expect("report");
    assert!(report.reused);
    assert_eq!(report.age_ms, 200);
    assert_eq!(tf_planned(&reply_d), plan_a);
    assert!(
        refused.should_skip(
            "w1",
            &desired_w1,
            &readback_w1,
            f.t0 + Duration::from_millis(1500)
        ),
        "clamp suppression survives renewed-base reuse"
    );
    // Same Hint again holds, then a genuine fresh change reallocates and
    // re-arms the clamp pair; a matching readback clears the lane.
    tf_fresh_at(
        &f,
        f.t0 + Duration::from_millis(1600),
        &[(f.keys[0].hwnd, wide)],
    );
    let (_, reply_e) = tf_tick(&mut f, &observed, "tf-e");
    assert_eq!(tf_planned(&reply_e), plan_a);
    let narrow = tf_hints(Some((616, 16)));
    assert_eq!(narrow.min_w, Some(600));
    tf_fresh_at(
        &f,
        f.t0 + Duration::from_millis(1700),
        &[(f.keys[0].hwnd, narrow)],
    );
    let (rows_g, reply_g) = tf_tick(&mut f, &observed, "tf-g");
    assert_eq!(tf_row_hints(&rows_g, "w1"), narrow);
    let plan_g = tf_planned(&reply_g);
    assert!(!plan_g.is_empty(), "genuine change replans");
    assert_ne!(plan_g, plan_a, "genuine change reallocates siblings");
    let desired_new = plan_g
        .iter()
        .find(|(token, _)| token == "w1")
        .expect("w1 target")
        .1;
    let at = f.t0 + Duration::from_millis(1700);
    assert!(
        !refused.should_skip("w1", &desired_new, &readback_w1, at),
        "genuine change re-arms the clamp pair"
    );
    refused.note_match("w1");
    assert!(
        !refused.should_skip("w1", &desired_w1, &readback_w1, at),
        "matching readback clears the lane"
    );
    tf_teardown();
}

#[test]
fn fresh_valid_change_replaces_and_updates_targets() {
    // A fresh usable hint replaces immediately: targets move to the new
    // allocation and a later Timeout reuses the new value, never the old.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let (_, reply_a) = tf_tick(&mut f, &observed, "tf-a");
    let plan_a = tf_planned(&reply_a);
    // Synthetic change: outer 616x16 = visible 600 wide. Still above the
    // fair share, below the old floor, so the allocation must move.
    let narrow = tf_hints(Some((616, 16)));
    assert_eq!(narrow.min_w, Some(600));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, narrow)]);
    let (rows_c, reply_c) = tf_tick(&mut f, &observed, "tf-c");
    assert_eq!(tf_row_hints(&rows_c, "w1"), narrow);
    let report = f.state.hint_fallback_report("w1").expect("fallback report");
    assert!(!report.reused, "fresh rows never report reuse");
    let plan_c = tf_planned(&reply_c);
    assert!(!plan_c.is_empty(), "changed hint replans");
    assert_ne!(plan_c, plan_a, "fresh change updates targets");
    // A later Timeout reuses the replaced value.
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let (rows_d, _) = tf_tick(&mut f, &observed, "tf-d");
    assert_eq!(tf_row_hints(&rows_d, "w1"), narrow);
    assert!(f.state.hint_fallback_report("w1").expect("report").reused);
    tf_teardown();
}

#[test]
fn timeout_reuse_has_no_time_bound() {
    // No time bound: a Timeout hours after the successful query still
    // reuses, with honest diagnostic age. Only a new successful query, a
    // close, or an identity change ends the entry.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let (_, reply_a) = tf_tick(&mut f, &observed, "tf-a");
    let plan_a = tf_planned(&reply_a);
    assert!(!plan_a.is_empty(), "seed tick plans geometry");
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    super::inject_hint_now(Some(f.t0 + Duration::from_secs(3 * 3600)));
    let (rows_b, reply_b) = tf_tick(&mut f, &observed, "tf-b");
    assert_eq!(tf_row_hints(&rows_b, "w1"), wide);
    let report = f.state.hint_fallback_report("w1").expect("report");
    assert!(report.reused);
    assert_eq!(report.age_ms, 3 * 3600 * 1000);
    assert_eq!(
        tf_planned(&reply_b),
        plan_a,
        "hours-old reuse holds targets"
    );
    super::inject_hint_now(Some(f.t0 + Duration::from_secs(5 * 3600)));
    let (rows_c, _) = tf_tick(&mut f, &observed, "tf-c");
    assert_eq!(tf_row_hints(&rows_c, "w1"), wide);
    let report = f.state.hint_fallback_report("w1").expect("report");
    assert!(report.reused);
    assert_eq!(report.age_ms, 5 * 3600 * 1000);
    assert!(f.state.hint_last_good.contains_key(&f.keys[0]));
    tf_teardown();
}

#[test]
fn successful_empty_clears_cache_and_timeout_cannot_revive() {
    // A successful query reporting no usable hint clears the stored entry:
    // a later Timeout stays unknown and cannot resurrect the previous value.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let _ = tf_tick(&mut f, &observed, "tf-a");
    assert!(f.state.hint_last_good.contains_key(&f.keys[0]));
    tf_inject_fresh(&f, &[]);
    let (rows_b, _) = tf_tick(&mut f, &observed, "tf-b");
    assert!(tf_row_hints(&rows_b, "w1").is_empty());
    assert!(
        !f.state.hint_last_good.contains_key(&f.keys[0]),
        "successful empty clears the entry"
    );
    let report = f.state.hint_fallback_report("w1").expect("report");
    assert!(!report.reused);
    assert_eq!(report.reason, "no-hint");
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let (rows_c, _) = tf_tick(&mut f, &observed, "tf-c");
    assert!(
        tf_row_hints(&rows_c, "w1").is_empty(),
        "Timeout cannot revive cleared hints"
    );
    let report = f.state.hint_fallback_report("w1").expect("report");
    assert!(!report.reused);
    assert_eq!(report.reason, "no-entry");
    tf_teardown();
}

#[test]
fn unperformed_invalid_preserves_cache_for_later_timeout() {
    // Identity/measurement failures never ran a query: the stored entry
    // survives an unperformed `Invalid` and a later Timeout still reuses it.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let _ = tf_tick(&mut f, &observed, "tf-a");
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Invalid);
    let (rows_b, _) = tf_tick(&mut f, &observed, "tf-b");
    assert!(tf_row_hints(&rows_b, "w1").is_empty());
    assert!(
        f.state.hint_last_good.contains_key(&f.keys[0]),
        "unperformed query preserves the entry"
    );
    let report = f.state.hint_fallback_report("w1").expect("report");
    assert!(!report.reused);
    assert_eq!(report.reason, "invalid");
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let (rows_c, _) = tf_tick(&mut f, &observed, "tf-c");
    assert_eq!(tf_row_hints(&rows_c, "w1"), wide);
    assert!(f.state.hint_fallback_report("w1").expect("report").reused);
    tf_teardown();
}

#[test]
fn failed_and_budget_stay_unknown_without_consuming_cache() {
    // Failed/Budget never turn into fallback rows, and they leave the stored
    // entry alone for a later Timeout to reuse.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let _ = tf_tick(&mut f, &observed, "tf-a");
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Failed);
    let (rows_b, _) = tf_tick(&mut f, &observed, "tf-b");
    assert!(tf_row_hints(&rows_b, "w1").is_empty());
    assert!(f.state.hint_last_good.contains_key(&f.keys[0]));
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Budget);
    let (rows_c, _) = tf_tick(&mut f, &observed, "tf-c");
    assert!(tf_row_hints(&rows_c, "w1").is_empty());
    assert!(f.state.hint_last_good.contains_key(&f.keys[0]));
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let (rows_d, _) = tf_tick(&mut f, &observed, "tf-d");
    assert_eq!(tf_row_hints(&rows_d, "w1"), wide);
    tf_teardown();
}

#[test]
fn new_identity_and_recycled_hwnd_cannot_reuse() {
    // Lifetime binding: a new member on the same HWND never inherits, a
    // same-process HWND reuse (live tag mismatch) never reuses, and a token
    // remint never reuses.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let _ = tf_tick(&mut f, &observed, "tf-a");
    // New member, same HWND, fresh token/tag: Timeout on the shared HWND
    // reuses for the bound member only.
    let key_new = crate::workspace::WindowKey {
        hwnd: f.keys[0].hwnd,
        pid: 9999,
        creation: "ctf-new-member".to_owned(),
    };
    assert!(
        f.state
            .workspaces
            .assign(key_new.clone(), &f.output, &f.ws, false)
    );
    f.state
        .member_tokens
        .insert(key_new.clone(), "w1b".to_owned());
    f.state.member_identity.insert(
        key_new.clone(),
        crate::model::ProcessIdentity {
            pid: key_new.pid,
            process_creation: key_new.creation.clone(),
            user_sid: "S-1-5-tf".to_owned(),
            session_id: 1,
            exe_path: "tf.exe".to_owned(),
        },
    );
    f.state
        .member_tags
        .insert(key_new.clone(), "tf-tag-w1b".to_owned());
    super::inject_pending_tag(key_new.hwnd, Some("tf-tag-w1".to_owned()));
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let mut observed_b = tf_observed_all(&f);
    observed_b.push(tf_observed("w1b", &key_new, tf_quarters()[0]));
    let (rows_b, _) = tf_tick(&mut f, &observed_b, "tf-b");
    assert_eq!(tf_row_hints(&rows_b, "w1"), wide);
    assert!(
        tf_row_hints(&rows_b, "w1b").is_empty(),
        "new member never inherits"
    );
    // Same-process HWND reuse: the live tag no longer matches the stored
    // tag while HWND/PID/creation still agree, so no reuse and the suspect
    // entry drops fail-closed.
    super::inject_pending_tag(f.keys[0].hwnd, Some("tf-tag-stale".to_owned()));
    let (rows_c, _) = tf_tick(&mut f, &observed_b, "tf-c");
    assert!(
        tf_row_hints(&rows_c, "w1").is_empty(),
        "recycled HWND never reuses"
    );
    assert!(
        !f.state.hint_last_good.contains_key(&f.keys[0]),
        "lifecycle mismatch drops the entry"
    );
    // A fresh usable query re-binds the lifecycle; a later Timeout reuses
    // the rebound value.
    super::clear_test_hints();
    super::inject_hint_now(Some(f.t0));
    super::inject_pending_tag(f.keys[0].hwnd, Some("tf-tag-w1".to_owned()));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let (rows_d, _) = tf_tick(&mut f, &observed_b, "tf-d");
    assert_eq!(tf_row_hints(&rows_d, "w1"), wide);
    assert!(
        !f.state.hint_fallback_report("w1").expect("report").reused,
        "fresh rows never report reuse"
    );
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let (rows_d2, _) = tf_tick(&mut f, &observed_b, "tf-d2");
    assert_eq!(tf_row_hints(&rows_d2, "w1"), wide);
    // Token remint on the same key: the stored token no longer matches.
    f.state
        .member_tokens
        .insert(f.keys[0].clone(), "w1r".to_owned());
    let mut observed_e = tf_observed_all(&f);
    for window in &mut observed_e {
        if window.token == "w1" {
            window.token = "w1r".to_owned();
        }
    }
    observed_e.push(tf_observed("w1b", &key_new, tf_quarters()[0]));
    let (rows_e, _) = tf_tick(&mut f, &observed_e, "tf-e");
    assert!(
        tf_row_hints(&rows_e, "w1r").is_empty(),
        "reminted token never reuses"
    );
    tf_teardown();
}

#[test]
fn close_drops_cache_and_readmission_cannot_reuse() {
    // Lifecycle prune: verified close drops the entry and its report, so a
    // later readmission with the same identity starts unknown.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let _ = tf_tick(&mut f, &observed, "tf-a");
    let key = f.keys[0].clone();
    super::drop_member_state(&mut f.state, &key);
    assert!(!f.state.hint_last_good.contains_key(&key));
    assert!(f.state.hint_fallback_report("w1").is_none());
    // Readmit the same identity with a fresh token/tag lifecycle.
    let key_back = tf_member(
        &mut f.state,
        &f.output.clone(),
        &f.ws.clone(),
        "w1",
        key.hwnd,
    );
    assert_eq!(key_back, key);
    super::inject_test_outcome(key.hwnd, super::HintOutcome::Timeout);
    tf_inject_fresh(&f, &[]);
    let observed_b = tf_observed_all(&f);
    let (rows_b, _) = tf_tick(&mut f, &observed_b, "tf-b");
    assert!(
        tf_row_hints(&rows_b, "w1").is_empty(),
        "readmission starts unknown"
    );
    tf_teardown();
}

#[test]
fn hidden_member_timeout_reuses_verified_row() {
    // Verified hidden member rows share the same Timeout reuse: the hidden
    // fences already hold (member identity plus lifetime tag populated), so
    // the row keeps its declared hint through a Timeout.
    let mut f = tf_setup();
    let tall = tf_hints(Some((16, 616)));
    assert_eq!(tall.min_h, Some(600));
    let hidden_key = f.keys[2].clone();
    f.state.workspaces.set_hidden(&hidden_key, true);
    let snapshot = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    f.state.member_rects.insert("w3".to_owned(), snapshot);
    tf_inject_fresh(&f, &[(hidden_key.hwnd, tall)]);
    let mut observed: Vec<super::ObservedWindow> = ["w1", "w2", "w4"]
        .iter()
        .zip([&f.keys[0], &f.keys[1], &f.keys[3]])
        .zip(tf_quarters().iter())
        .map(|((token, key), rect)| tf_observed(token, key, *rect))
        .collect();
    let _ = tf_tick(&mut f, &observed, "tf-a");
    // Tick B observes the same visible set; the hidden row re-queries.
    observed = ["w1", "w2", "w4"]
        .iter()
        .zip([&f.keys[0], &f.keys[1], &f.keys[3]])
        .zip(tf_quarters().iter())
        .map(|((token, key), rect)| tf_observed(token, key, *rect))
        .collect();
    super::inject_test_outcome(hidden_key.hwnd, super::HintOutcome::Timeout);
    let (rows_b, _) = tf_tick(&mut f, &observed, "tf-b");
    let row = rows_b.iter().find(|row| row.token == "w3").expect("w3 row");
    assert_eq!(row.hints, tall, "hidden Timeout keeps the declared hint");
    assert_eq!(row.rect, snapshot, "hidden snapshot rect untouched");
    assert!(!row.floating, "hidden row keeps its lane");
    let report = f.state.hint_fallback_report("w3").expect("fallback report");
    assert!(report.reused);
    tf_teardown();
}

#[test]
fn domain_release_preserves_live_member_last_good() {
    // Release never drops same-live-window entries: the cache is bound to
    // the native member lifetime, not the Engine domain, so a timing-out
    // member keeps its declared hint across the release.
    let mut f = tf_setup();
    let wide = tf_hints(Some((1016, 16)));
    tf_inject_fresh(&f, &[(f.keys[0].hwnd, wide)]);
    let observed = tf_observed_all(&f);
    let _ = tf_tick(&mut f, &observed, "tf-a");
    assert!(f.state.hint_last_good.contains_key(&f.keys[0]));
    let output = f.output.clone();
    let ws = f.ws.clone();
    super::reset_fixed_admission_for_domain(&mut f.state, &output, &ws);
    assert!(
        f.state.hint_last_good.contains_key(&f.keys[0]),
        "release preserves live-member entries"
    );
    super::inject_test_outcome(f.keys[0].hwnd, super::HintOutcome::Timeout);
    let (rows_b, _) = tf_tick(&mut f, &observed, "tf-b");
    assert_eq!(tf_row_hints(&rows_b, "w1"), wide);
    assert!(f.state.hint_fallback_report("w1").expect("report").reused);
    tf_teardown();
}
