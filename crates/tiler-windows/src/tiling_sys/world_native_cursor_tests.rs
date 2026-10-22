#![forbid(unsafe_code)]
//! Shared world-action acceptance over the production seam.
//!
//! Runs every case in `test-fixtures/world-actions-cursor.json` through the
//! real `LiveNative` actuator plus the real `WorldOwner` sequencing with a
//! scripted desktop (`live_tick_cursor` with `moving_prims`): `set_outer`
//! physically moves the observed frame, hide/reveal flip live visibility,
//! and the setter may lie. `FakeNative` is never the oracle here: arrivals
//! prove only through fresh frame plus visibility plus full identity reads,
//! exactly like the composition suite. Shared rig helpers (`env_with`,
//! `admit_bare`, `script_live`, `live_tick_cursor`, `moving_prims`,
//! `live_visible_observed`, `S_*` maps, `set_calls`) come from the parent
//! module via `use super::*`; cursor-case helpers live here alongside the
//! runner. Split from the composition suite as a focused child module.

use super::*;

fn fixture() -> serde_json::Value {
    let raw = include_str!("../../../../test-fixtures/world-actions-cursor.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid cursor fixture");
    assert_eq!(fixture["version"], serde_json::Value::from(1));
    fixture
}

fn scene_windows(fixture: &serde_json::Value) -> Vec<(String, String, Rect)> {
    fixture["scene"]["windows"]
        .as_array()
        .expect("scene windows")
        .iter()
        .map(|w| {
            let r = w["rect"].as_array().expect("scene rect");
            let nums: Vec<i32> = r
                .iter()
                .map(|v| v.as_i64().and_then(|n| i32::try_from(n).ok()).expect("i32"))
                .collect();
            (
                w["id"].as_str().expect("window id").to_owned(),
                w["output"].as_str().expect("window output").to_owned(),
                Rect {
                    x: nums[0],
                    y: nums[1],
                    w: nums[2],
                    h: nums[3],
                },
            )
        })
        .collect()
}

fn fixture_areas() -> (Vec<super::super::MonitorArea>, Vec<Rect>) {
    let areas = vec![
        super::super::MonitorArea {
            device: DEV1.to_owned(),
            work: rect(0, 0, 800, 560),
            full: rect(0, 0, 800, 600),
        },
        super::super::MonitorArea {
            device: DEV2.to_owned(),
            work: rect(800, 0, 800, 560),
            full: rect(800, 0, 800, 600),
        },
    ];
    let fulls = vec![rect(0, 0, 800, 600), rect(800, 0, 800, 600)];
    (areas, fulls)
}

fn device_for(scene_output: &str) -> &'static str {
    match scene_output {
        "out-a" => DEV1,
        "out-b" => DEV2,
        other => panic!("unknown scene output {other:?}"),
    }
}

fn cursor_of(case: &serde_json::Value) -> (i32, i32) {
    let cursor = case["cursor"].as_array().expect("cursor pair");
    (
        cursor[0].as_i64().expect("x") as i32,
        cursor[1].as_i64().expect("y") as i32,
    )
}

fn str_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("string list")
        .iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect()
}

fn union_box(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

fn live_frames() -> std::collections::HashMap<String, Rect> {
    live_visible_observed()
        .into_iter()
        .map(|w| (w.token, w.visible))
        .collect()
}

fn set_calls_for(hwnd: u64) -> usize {
    S_SET_CALLS.with(|m| m.borrow().iter().filter(|(h, _)| *h == hwnd).count())
}

fn view_of(reply: &WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

/// Admit the four scene windows with working hide/reveal claims, mirroring
/// the composition suite's follow-pair rig. Plants no applied receipts: the
/// seeding ticks must prove them.
fn admit_scene(env: &mut Env, scene: &[(String, String, Rect)], keys: &[(String, WindowKey)]) {
    for ((id, _, frame), (_, key)) in scene.iter().zip(keys.iter()) {
        admit_bare(env, key, id);
        script_live(key, id, *frame, plain_facts());
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
}

fn apply_targeting(env: &mut Env, case: &serde_json::Value) {
    let mut settings = crate::settings::Settings::default();
    settings.core.switch_target = case["settings"]["switch_target"]
        .as_str()
        .expect("switch_target")
        .to_owned();
    settings.core.fresh_placement = case["settings"]["fresh_placement"]
        .as_str()
        .expect("fresh_placement")
        .to_owned();
    env.state.settings_live = Some(crate::settings::LiveSettings::fresh(settings, None));
}

fn seed_ticks(
    env: &mut Env,
    owner: &mut WorldOwner,
    areas: &[super::super::MonitorArea],
    fulls: &[Rect],
    keys: &std::collections::HashMap<String, WindowKey>,
    case: &serde_json::Value,
) {
    let cursor = cursor_of(case);
    for focused in str_list(&case["seed_focus_order"])
        .iter()
        .chain(std::iter::once(
            &case["focus"].as_str().expect("focus").to_owned(),
        ))
    {
        let observed = live_visible_observed();
        live_tick_cursor(
            env,
            owner,
            &observed,
            &[],
            areas,
            fulls,
            moving_prims(),
            keys[focused].hwnd,
            None,
            Some(cursor),
        );
    }
}

fn case_keys(base: u64, scene: &[(String, String, Rect)]) -> Vec<(String, WindowKey)> {
    scene
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| {
            (
                id.clone(),
                win_key(
                    base + i as u64,
                    3000 + i as u32,
                    &format!("proc-{base}-{i}"),
                ),
            )
        })
        .collect()
}

#[test]
fn shared_fresh_cases_route_against_affinity_and_settle() {
    let fixture = fixture();
    let scene = scene_windows(&fixture);
    let (areas, fulls) = fixture_areas();
    let cases: Vec<&serde_json::Value> = fixture["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .filter(|c| c["kind"] == "fresh")
        .collect();
    assert!(!cases.is_empty(), "fixture carries fresh cases");
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().expect("case name");
        let mut env = env_with(310_000 + index as u64 * 100);
        env.state.ensure_device(DEV2);
        apply_targeting(&mut env, case);
        let wired: std::collections::HashMap<String, String> = ["out-a", "out-b"]
            .into_iter()
            .map(|out| (out.to_owned(), env.state.output_token(device_for(out))))
            .collect();
        let keys = case_keys(310_000 + index as u64 * 100, &scene);
        let by_id: std::collections::HashMap<String, WindowKey> = keys.iter().cloned().collect();
        admit_scene(&mut env, &scene, &keys);
        let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
        seed_ticks(&mut env, &mut owner, &areas, &fulls, &by_id, case);
        let cursor = cursor_of(case);
        let focus_hwnd = by_id[case["focus"].as_str().expect("focus")].hwnd;
        let baseline_frames = live_frames();
        let baseline_views = env.state.applied_views.clone();
        // Ordinary fresh window: physically created on the focused
        // (native-affinity) output, unfocused, while the cursor sits on the
        // other output. Policy must win over affinity.
        let newcomer = &case["newcomer"];
        let new_id = newcomer["id"].as_str().expect("newcomer id");
        let native_output = newcomer["native_output"].as_str().expect("native output");
        let newcomer_key = win_key(310_000 + index as u64 * 100 + 10, 3900, "proc-new");
        let native_rect = {
            let r = newcomer["rect"].as_array().expect("newcomer rect");
            rect(
                r[0].as_i64().expect("x") as i32,
                r[1].as_i64().expect("y") as i32,
                r[2].as_i64().expect("w") as i32,
                r[3].as_i64().expect("h") as i32,
            )
        };
        let _ = native_output;
        admit_bare(&mut env, &newcomer_key, new_id);
        script_live(&newcomer_key, new_id, native_rect, plain_facts());
        S_ADMIT.with(|m| {
            m.borrow_mut().insert(
                newcomer_key.hwnd,
                Ok(script_claim(newcomer_key.hwnd, &identity_of(&newcomer_key))),
            )
        });
        S_HIDE.with(|m| {
            m.borrow_mut().insert(
                newcomer_key.hwnd,
                Ok(valid_claim(newcomer_key.hwnd, &identity_of(&newcomer_key))),
            )
        });
        S_REVEAL.with(|m| {
            m.borrow_mut()
                .insert(newcomer_key.hwnd, Ok(Revealed::Shown))
        });
        let calls_before = set_calls_for(newcomer_key.hwnd);
        let observed = live_visible_observed();
        assert!(
            observed.iter().any(|w| w.token == new_id),
            "{name}: newcomer enumerates"
        );
        let tick = live_tick_cursor(
            &mut env,
            &mut owner,
            &observed,
            &[],
            &areas,
            &fulls,
            moving_prims(),
            focus_hwnd,
            None,
            Some(cursor),
        );
        let expect_output = case["expect"]["output"].as_str().expect("expect output");
        let anchor = case["expect"]["anchor"].as_str().expect("expect anchor");
        let stable = case["expect"]["stable_peer"].as_str().expect("stable peer");
        let wire = wired[expect_output].clone();
        // The reply publishes the newcomer slot on the policy output, and
        // the actuator physically lands it there: the live frame equals the
        // effective frame, never a setter echo.
        let slot = tick
            .reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == new_id)
            .and_then(|row| row.effective_frame)
            .unwrap_or_else(|| panic!("{name}: newcomer slot"));
        let after = live_frames();
        assert_eq!(
            after.get(new_id),
            Some(&slot),
            "{name}: live frame lands on the published slot"
        );
        assert_eq!(
            env.state.applied_homing.get(new_id),
            Some(&(wire.clone(), env.state.applied_views[&wire].clone())),
            "{name}: homing proves on the policy output"
        );
        assert!(
            set_calls_for(newcomer_key.hwnd) > calls_before,
            "{name}: newcomer physically moved"
        );
        // Relationship proof: exactly the anchor shrinks, the stable peer
        // and listed source windows keep their tiles, and newcomer plus
        // anchor refill the anchor's old tile.
        let anchor_before = baseline_frames[anchor];
        let anchor_after = after[anchor];
        assert!(
            anchor_after.w < anchor_before.w || anchor_after.h < anchor_before.h,
            "{name}: anchor {anchor} shrinks {anchor_before:?} -> {anchor_after:?}"
        );
        assert_eq!(
            after.get(stable),
            baseline_frames.get(stable),
            "{name}: stable peer {stable} unchanged"
        );
        assert_eq!(
            union_box(slot, anchor_after),
            anchor_before,
            "{name}: newcomer plus anchor refill the old tile"
        );
        for window in str_list(&case["expect"]["source_unchanged"]) {
            assert_eq!(
                after.get(&window),
                baseline_frames.get(&window),
                "{name}: source window {window} unchanged"
            );
        }
        // The target view proves through visibility, then the next
        // observation settles with no replay.
        assert_eq!(
            view_of(&tick.reply, &wire).as_deref(),
            env.state.applied_views.get(&wire).map(String::as_str),
            "{name}: target view proven"
        );
        let observed = live_visible_observed();
        let settled = live_tick_cursor(
            &mut env,
            &mut owner,
            &observed,
            &[],
            &areas,
            &fulls,
            moving_prims(),
            focus_hwnd,
            None,
            Some(cursor),
        );
        assert!(
            settled.reply.transfers.is_empty(),
            "{name}: settles with no flights: {:?}",
            settled.reply.transfers
        );
        assert!(
            settled.feedback.is_empty(),
            "{name}: no replay: {:?}",
            settled.feedback
        );
        assert_eq!(
            env.state.applied_homing.get(new_id),
            Some(&(wire.clone(), env.state.applied_views[&wire].clone())),
            "{name}: homing retained"
        );
        assert_eq!(
            env.state.applied_views.get(&wire),
            baseline_views
                .get(&wire)
                .or(env.state.applied_views.get(&wire)),
            "{name}: target view retained"
        );
    }
}

#[test]
fn shared_fresh_cases_cover_foreground_steal_and_loss() {
    // Live defect: a newcomer steals native foreground on arrival while the
    // seed focus stays behind, or the adapter reports empty/unknown focus
    // on the admission tick. Under focused-output-last-focused the retained
    // pre-admission focus must still route the newcomer; cursor placements
    // keep routing on the cursor. Each fresh case runs three admission
    // foregrounds: the old focus, the newcomer itself, and empty/unknown.
    let fixture = fixture();
    let scene = scene_windows(&fixture);
    let (areas, fulls) = fixture_areas();
    let cases: Vec<&serde_json::Value> = fixture["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .filter(|c| c["kind"] == "fresh")
        .collect();
    assert!(!cases.is_empty(), "fixture carries fresh cases");
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().expect("case name");
        let cursor = cursor_of(case);
        let newcomer = &case["newcomer"];
        let new_id = newcomer["id"].as_str().expect("newcomer id");
        let native_rect = {
            let r = newcomer["rect"].as_array().expect("newcomer rect");
            rect(
                r[0].as_i64().expect("x") as i32,
                r[1].as_i64().expect("y") as i32,
                r[2].as_i64().expect("w") as i32,
                r[3].as_i64().expect("h") as i32,
            )
        };
        let expect_output = case["expect"]["output"].as_str().expect("expect output");
        let anchor = case["expect"]["anchor"].as_str().expect("expect anchor");
        let stable = case["expect"]["stable_peer"].as_str().expect("stable peer");
        for (variant, focus_mode) in ["old", "steal", "empty", "unknown"].iter().enumerate() {
            let tag = format!("{name}-{focus_mode}");
            let base = 410_000 + (index as u64 * 4 + variant as u64) * 100;
            let mut env = env_with(base);
            env.state.ensure_device(DEV2);
            apply_targeting(&mut env, case);
            let wired: std::collections::HashMap<String, String> = ["out-a", "out-b"]
                .into_iter()
                .map(|out| (out.to_owned(), env.state.output_token(device_for(out))))
                .collect();
            let keys = case_keys(base, &scene);
            let by_id: std::collections::HashMap<String, WindowKey> =
                keys.iter().cloned().collect();
            admit_scene(&mut env, &scene, &keys);
            let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
            seed_ticks(&mut env, &mut owner, &areas, &fulls, &by_id, case);
            let baseline_frames = live_frames();
            let baseline_views = env.state.applied_views.clone();
            let newcomer_key = win_key(base + 10, 3900, "proc-new");
            admit_bare(&mut env, &newcomer_key, new_id);
            script_live(&newcomer_key, new_id, native_rect, plain_facts());
            S_ADMIT.with(|m| {
                m.borrow_mut().insert(
                    newcomer_key.hwnd,
                    Ok(script_claim(newcomer_key.hwnd, &identity_of(&newcomer_key))),
                )
            });
            S_HIDE.with(|m| {
                m.borrow_mut().insert(
                    newcomer_key.hwnd,
                    Ok(valid_claim(newcomer_key.hwnd, &identity_of(&newcomer_key))),
                )
            });
            S_REVEAL.with(|m| {
                m.borrow_mut()
                    .insert(newcomer_key.hwnd, Ok(Revealed::Shown))
            });
            let observed = live_visible_observed();
            assert!(
                observed.iter().any(|w| w.token == new_id),
                "{tag}: newcomer enumerates"
            );
            let foreground = match *focus_mode {
                "old" => by_id[case["focus"].as_str().expect("focus")].hwnd,
                "steal" => newcomer_key.hwnd,
                "empty" => 0,
                _ => 999_999,
            };
            let calls_before = set_calls_for(newcomer_key.hwnd);
            let tick = live_tick_cursor(
                &mut env,
                &mut owner,
                &observed,
                &[],
                &areas,
                &fulls,
                moving_prims(),
                foreground,
                None,
                Some(cursor),
            );
            let wire = wired[expect_output].clone();
            let slot = tick
                .reply
                .windows
                .iter()
                .find(|row| row.lifetime.0 == new_id)
                .and_then(|row| row.effective_frame)
                .unwrap_or_else(|| panic!("{tag}: newcomer slot"));
            let after = live_frames();
            assert_eq!(
                after.get(new_id),
                Some(&slot),
                "{tag}: live frame lands on the published slot"
            );
            assert_eq!(
                env.state.applied_homing.get(new_id),
                Some(&(wire.clone(), env.state.applied_views[&wire].clone())),
                "{tag}: homing proves on the policy output"
            );
            assert!(
                set_calls_for(newcomer_key.hwnd) > calls_before,
                "{tag}: newcomer physically moved"
            );
            let anchor_before = baseline_frames[anchor];
            let anchor_after = after[anchor];
            assert!(
                anchor_after.w < anchor_before.w || anchor_after.h < anchor_before.h,
                "{tag}: anchor {anchor} shrinks {anchor_before:?} -> {anchor_after:?}"
            );
            assert_eq!(
                after.get(stable),
                baseline_frames.get(stable),
                "{tag}: stable peer {stable} unchanged"
            );
            assert_eq!(
                union_box(slot, anchor_after),
                anchor_before,
                "{tag}: newcomer plus anchor refill the old tile"
            );
            for window in str_list(&case["expect"]["source_unchanged"]) {
                assert_eq!(
                    after.get(&window),
                    baseline_frames.get(&window),
                    "{tag}: source window {window} unchanged"
                );
            }
            assert_eq!(
                view_of(&tick.reply, &wire).as_deref(),
                env.state.applied_views.get(&wire).map(String::as_str),
                "{tag}: target view proven"
            );
            // The i3/ba empty-focus admission is the live failure: it must
            // resolve through the retained focus leg, not the primary
            // fallback.
            if *focus_mode == "empty"
                && case["settings"]["fresh_placement"] == "focused-output-last-focused"
            {
                let trace = owner.engine().fresh_focus_trace();
                assert_eq!(trace.3, "retained", "{tag}: retained leg wins");
                assert_eq!(trace.2.as_deref(), Some(wire.as_str()), "{tag}: leg output");
            }
            let observed = live_visible_observed();
            let settled = live_tick_cursor(
                &mut env,
                &mut owner,
                &observed,
                &[],
                &areas,
                &fulls,
                moving_prims(),
                foreground,
                None,
                Some(cursor),
            );
            assert!(
                settled.reply.transfers.is_empty(),
                "{tag}: settles with no flights: {:?}",
                settled.reply.transfers
            );
            assert!(
                settled.feedback.is_empty(),
                "{tag}: no replay: {:?}",
                settled.feedback
            );
            assert_eq!(
                env.state.applied_homing.get(new_id),
                Some(&(wire.clone(), env.state.applied_views[&wire].clone())),
                "{tag}: homing retained"
            );
            assert_eq!(
                env.state.applied_views.get(&wire),
                baseline_views
                    .get(&wire)
                    .or(env.state.applied_views.get(&wire)),
                "{tag}: target view retained"
            );
        }
    }
}

#[test]
fn shared_switch_and_send_cases_resolve_per_fixture() {
    let fixture = fixture();
    let scene = scene_windows(&fixture);
    let (areas, fulls) = fixture_areas();
    let cases: Vec<&serde_json::Value> = fixture["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .filter(|c| c["kind"] != "fresh")
        .collect();
    assert!(!cases.is_empty(), "fixture carries switch/send cases");
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().expect("case name");
        let mut env = env_with(320_000 + index as u64 * 100);
        env.state.ensure_device(DEV2);
        apply_targeting(&mut env, case);
        let wired: std::collections::HashMap<String, String> = ["out-a", "out-b"]
            .into_iter()
            .map(|out| (out.to_owned(), env.state.output_token(device_for(out))))
            .collect();
        let keys = case_keys(320_000 + index as u64 * 100, &scene);
        let by_id: std::collections::HashMap<String, WindowKey> = keys.iter().cloned().collect();
        admit_scene(&mut env, &scene, &keys);
        let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
        seed_ticks(&mut env, &mut owner, &areas, &fulls, &by_id, case);
        let cursor = cursor_of(case);
        let focus_hwnd = by_id[case["focus"].as_str().expect("focus")].hwnd;
        let baseline_views = env.state.applied_views.clone();
        let command = &case["command"];
        match case["kind"].as_str().expect("kind") {
            "switch" => {
                let expect_output = case["expect"]["output"].as_str().expect("expect output");
                let other = case["expect"]["other_output"].as_str().expect("other");
                let wire = wired[expect_output].clone();
                let other_wire = wired[other].clone();
                let before = baseline_views[&wire].clone();
                let observed = live_visible_observed();
                let tick = live_tick_cursor(
                    &mut env,
                    &mut owner,
                    &observed,
                    &[],
                    &areas,
                    &fulls,
                    moving_prims(),
                    focus_hwnd,
                    Some(WorldCommand::SelectTrailing { output: None }),
                    Some(cursor),
                );
                assert_ne!(
                    env.state.applied_views.get(&wire),
                    Some(&before),
                    "{name}: trailing lands on {expect_output}"
                );
                assert_eq!(
                    env.state.applied_views.get(&other_wire),
                    baseline_views.get(&other_wire),
                    "{name}: {other} untouched"
                );
                assert!(
                    tick.reply
                        .transfers
                        .iter()
                        .any(|t| t.target_output.0 == wire),
                    "{name}: view flight names {expect_output}: {:?}",
                    tick.reply.transfers
                );
                let observed = live_visible_observed();
                let settled = live_tick_cursor(
                    &mut env,
                    &mut owner,
                    &observed,
                    &[],
                    &areas,
                    &fulls,
                    moving_prims(),
                    focus_hwnd,
                    None,
                    Some(cursor),
                );
                assert!(
                    settled.reply.transfers.is_empty(),
                    "{name}: settles: {:?}",
                    settled.reply.transfers
                );
            }
            "send" => {
                let mover = command["window"].as_str().expect("mover");
                let mover_out = scene
                    .iter()
                    .find(|(id, _, _)| id == mover)
                    .map(|(_, output, _)| output.clone())
                    .expect("mover scene output");
                assert_eq!(
                    case["expect"]["output"].as_str().expect("expect output"),
                    mover_out,
                    "{name}: send stays on the mover output"
                );
                let wire = wired[&mover_out].clone();
                let observed = live_visible_observed();
                let tick = live_tick_cursor(
                    &mut env,
                    &mut owner,
                    &observed,
                    &[],
                    &areas,
                    &fulls,
                    moving_prims(),
                    by_id[mover].hwnd,
                    Some(WorldCommand::SendNumbered {
                        window: WindowId(mover.to_owned()),
                        output: OutputId(wire.clone()),
                        index: command["index"].as_u64().expect("index") as u8,
                        follow: command["follow"].as_bool().expect("follow"),
                    }),
                    Some(cursor),
                );
                let op = window_op(&tick.reply, mover);
                assert!(
                    tick.reply
                        .transfers
                        .iter()
                        .all(|t| t.target_output.0 != wire
                            || t.operation_id == op
                            || !matches!(&t.kind, TransferKind::Window(w) if w.0 == mover)),
                    "{name}: no off-output send: {:?}",
                    tick.reply.transfers
                );
                assert!(
                    applied_of(&tick.feedback, &op),
                    "{name}: send applies: {:?}",
                    tick.feedback
                );
                assert_eq!(
                    env.state.applied_homing.get(mover).map(|(o, _)| o),
                    Some(&wire),
                    "{name}: mover homed on its own output"
                );
                let observed = live_visible_observed();
                let settled = live_tick_cursor(
                    &mut env,
                    &mut owner,
                    &observed,
                    &[],
                    &areas,
                    &fulls,
                    moving_prims(),
                    by_id[mover].hwnd,
                    None,
                    Some(cursor),
                );
                assert!(
                    settled.reply.transfers.is_empty(),
                    "{name}: settles: {:?}",
                    settled.reply.transfers
                );
            }
            other => panic!("unknown case kind {other:?}"),
        }
    }
}
