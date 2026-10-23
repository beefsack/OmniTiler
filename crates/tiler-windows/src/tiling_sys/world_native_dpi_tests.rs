#![forbid(unsafe_code)]
//! Cross-DPI mechanical transition regressions over the production seam.
//!
//! Extracted from the parent module at the file-size review; shared rig
//! helpers and the scripted natives come from the parent via `use super::*;`.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;
// ---------------------------------------------------------------------------
// Cross-DPI mechanical transition regressions (native Windows defect 2c).
//
// A cross-monitor `SetWindowPos` changes the window's DPI under the call and
// the app framework synchronously rescales the frame to preserve logical
// size (winit `WM_DPICHANGED`: new_physical = old_physical * new/old with
// the position kept). The production frame path must re-observe fresh and
// reassert the same intended visible rectangle through the fresh insets
// once, under the same gates, so the transfer proves Applied in one tick
// instead of pinning a false clamp. Both DPI directions, plus identity
// drift and racing-fullscreen vetoes across the correction.
// ---------------------------------------------------------------------------

const DPI_120: u32 = 120;
const DPI_96: u32 = 96;

/// Live-measured invisible borders per DPI (DWM extended-frame deltas).
fn insets_for_dpi(dpi: u32) -> FrameInsets {
    match dpi {
        DPI_120 => FrameInsets {
            left: 8,
            top: 0,
            right: 8,
            bottom: 8,
        },
        _ => FrameInsets {
            left: 7,
            top: 0,
            right: 7,
            bottom: 7,
        },
    }
}

/// Script the two-output defect topology: DEV1 at 120 DPI on the left,
/// DEV2 at 96 DPI on the right.
fn cross_dpi_monitors() {
    S_MONITORS.with(|m| {
        *m.borrow_mut() = vec![
            (rect(0, 0, 1920, 1080), DPI_120),
            (rect(1920, 0, 1920, 1080), DPI_96),
        ]
    });
}

fn cross_dpi_areas() -> Vec<super::super::super::MonitorArea> {
    vec![
        super::super::super::MonitorArea {
            device: DEV1.to_owned(),
            work: rect(0, 0, 1920, 1040),
            full: rect(0, 0, 1920, 1080),
        },
        super::super::super::MonitorArea {
            device: DEV2.to_owned(),
            work: rect(1920, 0, 1920, 1040),
            full: rect(1920, 0, 1920, 1080),
        },
    ]
}

/// `set_outer` modeling the winit `WM_DPICHANGED` rescale: a write landing
/// on another monitor's DPI synchronously rescales the observed visible size
/// by new/old (position kept, insets re-measured at the new DPI);
/// same-DPI writes apply exactly. Refusals still behave per flag.
fn s_set_outer_cross_dpi(hwnd: u64, outer: Rect) -> bool {
    S_SET_CALLS.with(|m| m.borrow_mut().push((hwnd, outer)));
    if !S_SET_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true)) {
        return false;
    }
    let before = S_DPI.with(|m| m.borrow().get(&hwnd).copied());
    let after = {
        let cx = outer.x + outer.w / 2;
        let cy = outer.y + outer.h / 2;
        S_MONITORS.with(|m| {
            m.borrow()
                .iter()
                .find(|(full, _)| {
                    cx >= full.x && cx < full.x + full.w && cy >= full.y && cy < full.y + full.h
                })
                .map(|(_, dpi)| *dpi)
        })
    };
    S_OBSERVE.with(|m| {
        let mut map = m.borrow_mut();
        let Some(w) = map.get_mut(&hwnd) else {
            return;
        };
        // Intended visible under the pre-write frame.
        let vis = Rect {
            x: outer.x + w.insets.left,
            y: outer.y + w.insets.top,
            w: outer.w - w.insets.left - w.insets.right,
            h: outer.h - w.insets.top - w.insets.bottom,
        };
        match (before, after) {
            (Some(dpi_before), Some(dpi_after)) if dpi_before != dpi_after => {
                let scaled = Rect {
                    x: vis.x,
                    y: vis.y,
                    w: vis.w * dpi_after as i32 / dpi_before as i32,
                    h: vis.h * dpi_after as i32 / dpi_before as i32,
                };
                let insets = insets_for_dpi(dpi_after);
                w.outer = Rect {
                    x: scaled.x - insets.left,
                    y: scaled.y - insets.top,
                    w: scaled.w + insets.left + insets.right,
                    h: scaled.h + insets.top + insets.bottom,
                };
                w.visible = scaled;
                w.insets = insets;
                S_DPI.with(|d| {
                    d.borrow_mut().insert(hwnd, dpi_after);
                });
            }
            _ => {
                w.visible = vis;
                w.outer = outer;
                w.insets = FrameInsets::measure(outer, vis);
            }
        }
    });
    true
}

/// Script one live window with DPI-measured insets plus the DPI lane.
fn script_live_dpi(key: &WindowKey, token: &str, vis: Rect, dpi: u32) {
    let insets = insets_for_dpi(dpi);
    let outer = Rect {
        x: vis.x - insets.left,
        y: vis.y - insets.top,
        w: vis.w + insets.left + insets.right,
        h: vis.h + insets.top + insets.bottom,
    };
    S_OBSERVE.with(|m| {
        m.borrow_mut().insert(
            key.hwnd,
            super::super::super::ObservedWindow {
                hwnd: key.hwnd,
                token: token.to_owned(),
                outer,
                visible: vis,
                insets,
                identity: ObservedTarget {
                    hwnd: key.hwnd,
                    pid: key.pid,
                    process_creation: key.creation.clone(),
                    exe_path: EXE.to_owned(),
                    user_sid: "S-1-5-21-1000".to_owned(),
                    session_id: 1,
                    tag: String::new(),
                },
                facts: plain_facts(),
                member_tag: Some("member-tag-1".to_owned()),
            },
        )
    });
    S_PID.with(|m| m.borrow_mut().insert(key.hwnd, key.pid));
    S_VISIBLE.with(|m| m.borrow_mut().insert(key.hwnd));
    S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
    S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
    S_DPI.with(|m| m.borrow_mut().insert(key.hwnd, dpi));
}

/// Cross-DPI prims: every native lane scripted, frames behind the
/// rescaling desktop.
fn cross_dpi_prims() -> NativePrimitives {
    let mut prims = scripted();
    prims.set_outer = s_set_outer_cross_dpi;
    prims
}

/// Assemble the production actuator with caller-chosen desired/proven maps
/// for a two-output transfer (the shared helper mirrors proven everywhere).
#[allow(clippy::too_many_arguments)]
fn assemble_cross<'e>(
    env: &'e mut Env,
    prims: NativePrimitives,
    areas: Vec<super::super::super::MonitorArea>,
    homing: HashMap<String, (String, String)>,
    rings: HashMap<String, Vec<String>>,
    views: HashMap<String, String>,
    frames: HashMap<String, Rect>,
) -> LiveNative<'e> {
    LiveNative::assemble(
        &env.me,
        &env.fulls,
        &env.state.output_tokens,
        homing,
        rings,
        views,
        frames,
        areas,
        env.state.applied_homing.clone(),
        env.state.applied_views.clone(),
        &env.state.member_tokens,
        &env.state.member_tags,
        &env.state.member_identity,
        &mut env.state.hidden_claims,
        &env.state.sticky,
        &env.state.float_topmost_prev,
        &env.state.scope,
        &env.state.scope_hosts,
        None,
        false,
        &env.store,
        &env.dir,
        &env.state.active,
        prims,
    )
}

struct CrossRig {
    env: Env,
    areas: Vec<super::super::super::MonitorArea>,
    key: WindowKey,
    src_out: String,
    src_ws: String,
    tgt_out: String,
    tgt_ws: String,
}

/// One member homed on `src` (`"o1"` or `"o2"`, the other is the target)
/// with the defect topology scripted.
fn cross_rig(base_hwnd: u64, start: Rect, start_dpi: u32, src: &str) -> CrossRig {
    let mut env = env_with(base_hwnd);
    env.state.ensure_device(DEV1);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (src_out, tgt_out) = if src == "o2" {
        (o2.clone(), o1.clone())
    } else {
        (o1.clone(), o2.clone())
    };
    cross_dpi_monitors();
    let areas = cross_dpi_areas();
    env.fulls = vec![rect(0, 0, 1920, 1080), rect(1920, 0, 1920, 1080)];
    let key = win_key(base_hwnd, 2301, "proc-301");
    env.state.member_tokens.insert(key.clone(), "w1".to_owned());
    env.state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    env.state
        .member_identity
        .insert(key.clone(), identity_of(&key));
    env.state
        .applied_homing
        .insert("w1".to_owned(), (src_out.clone(), "wsa".to_owned()));
    env.state
        .applied_views
        .insert(src_out.clone(), "wsa".to_owned());
    script_live_dpi(&key, "w1", start, start_dpi);
    S_FG.with(|m| *m.borrow_mut() = key.hwnd);
    CrossRig {
        env,
        areas,
        key,
        src_out: src_out.clone(),
        src_ws: "wsa".to_owned(),
        tgt_out: tgt_out.clone(),
        tgt_ws: "wsb".to_owned(),
    }
}

/// Scripted racing game foreground for the veto test: when set, the
/// cross-DPI fake raises a fullscreen foreground mid-call.
fn s_set_outer_cross_dpi_with_game_race(hwnd: u64, outer: Rect) -> bool {
    let moved = s_set_outer_cross_dpi(hwnd, outer);
    if let Some(game) = S_GAME_FG.with(|m| *m.borrow()) {
        let mut facts = plain_facts();
        facts.captionless_fullscreen = true;
        S_OBSERVE.with(|m| {
            m.borrow_mut().insert(
                game,
                super::super::super::ObservedWindow {
                    hwnd: game,
                    token: "fg".to_owned(),
                    outer: rect(1920, 0, 1920, 1080),
                    visible: rect(1920, 0, 1920, 1080),
                    insets: FrameInsets::measure(
                        rect(1920, 0, 1920, 1080),
                        rect(1920, 0, 1920, 1080),
                    ),
                    identity: ObservedTarget {
                        hwnd: game,
                        pid: 2302,
                        process_creation: "proc-302".to_owned(),
                        exe_path: EXE.to_owned(),
                        user_sid: "S-1-5-21-1000".to_owned(),
                        session_id: 1,
                        tag: String::new(),
                    },
                    facts,
                    member_tag: Some("member-tag-1".to_owned()),
                },
            )
        });
        S_FG.with(|m| *m.borrow_mut() = game);
    }
    moved
}

/// Tiled transfer reply moving `w1` to the rig target with frame `target`.
fn cross_reply(rig: &CrossRig, target: Rect) -> WorldReply {
    let mut reply = reply_shell(
        7,
        1,
        &[
            (rig.src_out.as_str(), rig.src_ws.as_str()),
            (rig.tgt_out.as_str(), rig.tgt_ws.as_str()),
        ],
    );
    reply.windows.push(WindowDesire {
        lifetime: WindowId("w1".to_owned()),
        output: OutputId(rig.tgt_out.clone()),
        workspace: rig.tgt_ws.clone(),
        allocation: Some(target),
        effective_frame: Some(target),
        frame_authority: Auth::Controlled,
        membership_authority: Auth::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: None,
        clear_operation: None,
    });
    reply.transfers.push(TransferTarget {
        operation_id: "op-x".to_owned(),
        kind: TransferKind::Window(WindowId("w1".to_owned())),
        target_output: OutputId(rig.tgt_out.clone()),
        target_workspace: rig.tgt_ws.clone(),
        follow: false,
        held: false,
    });
    reply.outputs.push(tiler_core::world_report::OutputDesire {
        output: OutputId(rig.tgt_out.clone()),
        logical_view: Some(rig.tgt_ws.clone()),
        tiling_mode: tiler_core::world_report::TilingMode::Tiled,
        hold: None,
    });
    reply
}

fn cross_native<'e>(
    rig: &'e mut CrossRig,
    prims: NativePrimitives,
    target: Rect,
) -> (LiveNative<'e>, FreshHost, HostDiff) {
    let fg = rig.key.hwnd;
    let inputs = rig.env.state.inputs();
    let observed = live_visible_observed();
    let areas = rig.areas.clone();
    let captured = super::super::super::world_capture::capture_world(
        &inputs,
        Some(&observed),
        &[],
        &areas,
        fg,
    );
    assert!(captured.inventory_complete);
    let fresh = FreshHost::from_sources(&inputs, &observed, &[], &captured);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let mut homing = HashMap::new();
    homing.insert("w1".to_owned(), (rig.tgt_out.clone(), rig.tgt_ws.clone()));
    let mut rings = HashMap::new();
    rings.insert(rig.src_out.clone(), vec![rig.src_ws.clone()]);
    rings.insert(rig.tgt_out.clone(), vec![rig.tgt_ws.clone()]);
    let mut views = HashMap::new();
    views.insert(rig.src_out.clone(), rig.src_ws.clone());
    views.insert(rig.tgt_out.clone(), rig.tgt_ws.clone());
    let mut frames = HashMap::new();
    frames.insert("w1".to_owned(), target);
    let native = assemble_cross(&mut rig.env, prims, areas, homing, rings, views, frames);
    (native, fresh, diff)
}

/// Live defect direction one (ab): 120-DPI source, 96-DPI target. The first
/// write lands short by 96/120 until the fresh reassertion corrects it.
#[test]
fn cross_dpi_shrink_arrives_exact_and_proves() {
    let mut rig = cross_rig(301_001, rect(100, 100, 800, 600), DPI_120, "o1");
    let target = rect(2000, 100, 960, 520);
    let hwnd = rig.key.hwnd;
    let tgt = (rig.tgt_out.clone(), rig.tgt_ws.clone());
    let reply = cross_reply(&rig, target);
    let (mut native, fresh, mut diff) = cross_native(&mut rig, cross_dpi_prims(), target);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(
        applied_of_members(&members, &views, "op-x"),
        "shrink transfer applies: {members:?} {views:?} {stats:?}"
    );
    assert_eq!(stats.membership_writes, 1, "{stats:?}");
    assert_eq!(stats.held, 0, "{stats:?}");
    assert!(!stats.partial, "{stats:?}");
    assert_eq!(S_DPI.with(|m| m.borrow().get(&hwnd).copied()), Some(DPI_96));
    let live = live_visible_observed();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].visible, target, "effective frame lands exact");
    // One bounded correction: source-inset outer, then fresh-inset outer.
    assert_eq!(
        S_SET_CALLS.with(|m| m.borrow().clone()),
        vec![
            (hwnd, rect(1992, 100, 976, 528)),
            (hwnd, rect(1993, 100, 974, 527)),
        ]
    );
    let (homing, _) = native.receipt_updates(&reply);
    assert_eq!(
        homing.get("w1"),
        Some(&tgt),
        "arrival receipt proves: {homing:?}"
    );
}

/// Live defect direction two (ba): 96-DPI source, 120-DPI target. The first
/// write lands oversized by 120/96 until the fresh reassertion corrects it.
#[test]
fn cross_dpi_grow_arrives_exact_and_proves() {
    let mut rig = cross_rig(301_101, rect(2000, 100, 768, 416), DPI_96, "o2");
    let target = rect(100, 100, 960, 520);
    let hwnd = rig.key.hwnd;
    let tgt = (rig.tgt_out.clone(), rig.tgt_ws.clone());
    let reply = cross_reply(&rig, target);
    let (mut native, fresh, mut diff) = cross_native(&mut rig, cross_dpi_prims(), target);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(
        applied_of_members(&members, &views, "op-x"),
        "grow transfer applies: {members:?} {views:?} {stats:?}"
    );
    assert_eq!(stats.membership_writes, 1, "{stats:?}");
    assert_eq!(stats.held, 0, "{stats:?}");
    let live = live_visible_observed();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].visible, target, "effective frame lands exact");
    assert_eq!(
        S_SET_CALLS.with(|m| m.borrow().clone()),
        vec![
            (hwnd, rect(93, 100, 974, 527)),
            (hwnd, rect(92, 100, 976, 528)),
        ]
    );
    let (homing, _) = native.receipt_updates(&reply);
    assert_eq!(
        homing.get("w1"),
        Some(&tgt),
        "arrival receipt proves: {homing:?}"
    );
}

/// Identity drift across the transition stays ambiguous: the recycled HWND
/// fails before any correction, so the flight arms at-most-once and never
/// fabricates arrival on retry.
#[test]
fn cross_dpi_identity_drift_holds_ambiguous_without_correction() {
    let mut rig = cross_rig(301_201, rect(100, 100, 800, 600), DPI_120, "o1");
    let target = rect(2000, 100, 960, 520);
    let tgt = (rig.tgt_out.clone(), rig.tgt_ws.clone());
    let reply = cross_reply(&rig, target);
    let mut prims = cross_dpi_prims();
    prims.set_outer = |hwnd, outer| {
        let moved = s_set_outer_cross_dpi(hwnd, outer);
        // The HWND recycles under the same number mid-call: new pid and
        // creation, so both the pid lane and the identity lane drift.
        S_PID.with(|m| m.borrow_mut().insert(hwnd, 9999));
        S_IDENT.with(|m| {
            m.borrow_mut().insert(
                hwnd,
                ProcessIdentity {
                    pid: 9999,
                    process_creation: "proc-999".to_owned(),
                    user_sid: "S-1-5-21-1000".to_owned(),
                    session_id: 1,
                    exe_path: EXE.to_owned(),
                },
            )
        });
        moved
    };
    let (mut native, fresh, mut diff) = cross_native(&mut rig, prims, target);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(
        members.iter().any(|a| a.operation_id == "op-x"),
        "drifted transfer holds ambiguous: {members:?} {views:?}"
    );
    assert!(
        !applied_of_members(&members, &views, "op-x"),
        "no fabricated arrival: {members:?} {views:?}"
    );
    assert_eq!(set_calls(), 1, "no correction into drifted identity");
    assert_eq!(stats.held, 1, "{stats:?}");
    assert_ne!(
        native.read_homing("w1"),
        Some(tgt),
        "drifted homing never proves arrival"
    );
    let (homing, _) = native.receipt_updates(&reply);
    assert!(homing.is_empty(), "no receipt from drift: {homing:?}");
    // The spent flight retries silently without new writes or resolution.
    let (mut native, fresh, _) = cross_native(&mut rig, cross_dpi_prims(), target);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(stats.held > 0);
    assert!(
        members.is_empty() && views.is_empty(),
        "spent flight holds silent"
    );
    assert_eq!(set_calls(), 1, "no repeat writes");
}

/// A racing fullscreen foreground across the transition vetoes the
/// correction: the first write stands, no second write fires into the game,
/// and the flight holds ambiguous without arrival proof.
#[test]
fn cross_dpi_racing_fullscreen_vetoes_correction() {
    let mut rig = cross_rig(301_301, rect(100, 100, 800, 600), DPI_120, "o1");
    let target = rect(2000, 100, 960, 520);
    let reply = cross_reply(&rig, target);
    let game = win_key(301_302, 2302, "proc-302").hwnd;
    S_GAME_FG.with(|m| *m.borrow_mut() = Some(game));
    let mut prims = cross_dpi_prims();
    prims.set_outer = s_set_outer_cross_dpi_with_game_race;
    let (mut native, fresh, mut diff) = cross_native(&mut rig, prims, target);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(
        members.iter().any(|a| a.operation_id == "op-x"),
        "vetoed transfer holds ambiguous: {members:?} {views:?}"
    );
    assert!(!applied_of_members(&members, &views, "op-x"));
    assert_eq!(stats.held, 1, "{stats:?}");
    assert_eq!(set_calls(), 1, "no correction into fullscreen race");
    assert!(
        S_SET_CALLS.with(|m| m.borrow().iter().all(|(hwnd, _)| *hwnd != game)),
        "game surface untouched"
    );
    let (homing, _) = native.receipt_updates(&reply);
    assert!(homing.is_empty(), "no receipt under veto: {homing:?}");
}

// Fresh-facts override for the ineligibility test: when set, the
// cross-DPI fake rewrites the observed facts mid-call.
type IneligibleCase = (&'static str, fn(&mut WindowFacts), u64);

thread_local! {
    static S_INELIGIBLE: RefCell<Option<fn(&mut WindowFacts)>> = const { RefCell::new(None) };
}

/// Cross-DPI fake variant that additionally turns the freshly observed
/// facts ineligible mid-call (minimized, cloaked, or hidden).
fn s_set_outer_cross_dpi_with_ineligible_facts(hwnd: u64, outer: Rect) -> bool {
    let moved = s_set_outer_cross_dpi(hwnd, outer);
    if let Some(make) = S_INELIGIBLE.with(|m| *m.borrow()) {
        S_OBSERVE.with(|m| {
            if let Some(w) = m.borrow_mut().get_mut(&hwnd) {
                make(&mut w.facts);
            }
        });
    }
    moved
}

/// Freshly ineligible frames across the transition take no corrective
/// setter: a minimized, cloaked, or hidden arrival each holds with exactly
/// one write, no correction, and no arrival proof.
#[test]
fn cross_dpi_freshly_ineligible_takes_no_correction() {
    fn ineligible_minimized(facts: &mut WindowFacts) {
        facts.minimized = true;
    }
    fn ineligible_cloaked(facts: &mut WindowFacts) {
        facts.cloaked = true;
    }
    fn ineligible_hidden(facts: &mut WindowFacts) {
        facts.visible = false;
    }
    let cases: [IneligibleCase; 3] = [
        ("minimized", ineligible_minimized, 302_411),
        ("cloaked", ineligible_cloaked, 302_412),
        ("hidden", ineligible_hidden, 302_413),
    ];
    for (name, make, base) in cases {
        S_INELIGIBLE.with(|m| *m.borrow_mut() = None);
        let mut rig = cross_rig(base, rect(100, 100, 800, 600), DPI_120, "o1");
        let target = rect(2000, 100, 960, 520);
        let reply = cross_reply(&rig, target);
        S_INELIGIBLE.with(|m| *m.borrow_mut() = Some(make));
        let mut prims = cross_dpi_prims();
        prims.set_outer = s_set_outer_cross_dpi_with_ineligible_facts;
        let (mut native, fresh, mut diff) = cross_native(&mut rig, prims, target);
        let mut members = Vec::new();
        let mut views = Vec::new();
        let stats = apply_world_reply(
            &mut diff,
            &reply,
            &fresh,
            &mut native,
            &mut Vec::new(),
            &mut members,
            &mut views,
            &mut Vec::new(),
            0,
        );
        assert!(
            members.iter().any(|a| a.operation_id == "op-x"),
            "{name}: ineligible transfer holds ambiguous: {members:?} {views:?}"
        );
        assert!(
            !applied_of_members(&members, &views, "op-x"),
            "{name}: no fabricated arrival"
        );
        assert_eq!(set_calls(), 1, "{name}: no corrective setter");
        assert_eq!(stats.held, 1, "{name}: {stats:?}");
        let live = live_visible_observed();
        assert_eq!(live.len(), 1);
        assert_ne!(
            live[0].visible, target,
            "{name}: rescaled frame never corrected"
        );
        let (homing, _) = native.receipt_updates(&reply);
        assert!(homing.is_empty(), "{name}: no receipt: {homing:?}");
    }
}
