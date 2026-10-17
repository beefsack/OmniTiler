#![forbid(unsafe_code)]
use super::*;

fn key(hwnd: u64) -> WindowKey {
    WindowKey {
        hwnd,
        pid: 1000 + hwnd as u32,
        creation: format!("c{hwnd:016x}"),
    }
}

fn order_ids(m: &ManagedWorkspaces, output: &str) -> Vec<String> {
    m.outputs
        .get(output)
        .map(|s| s.order.iter().map(|e| e.id.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn decode_digit_alias_and_modifiers() {
    assert_eq!(
        decode_digit(VK_0 + 1, false, false, false, true),
        Some((DigitOp::Select, 1))
    );
    assert_eq!(
        decode_digit(VK_0 + 1, true, false, false, true),
        Some((DigitOp::Send, 1))
    );
    assert_eq!(decode_digit(VK_0 + 1, false, true, false, true), None);
    assert_eq!(decode_digit(VK_0 + 1, false, false, true, true), None);
    assert_eq!(decode_digit(VK_0 + 1, false, false, false, false), None);
    assert_eq!(decode_digit(0x41, false, false, false, true), None);
    assert!(is_digit_vk(VK_0) && is_digit_vk(VK_9));
    assert!(!is_digit_vk(0x41));
}

#[test]
fn select_existing_never_creates() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    assert_eq!(m.workspace_count("mon-1"), 2);
    let first = m.select("mon-1", 1).expect("select 1");
    assert_eq!(m.active_id("mon-1").as_deref(), Some(first.as_str()));
    assert!(m.select("mon-1", 9).is_none());
    assert_eq!(m.workspace_count("mon-1"), 2);
}

#[test]
fn trailing_zero_reuses_then_creates() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let (id, created) = m.select_trailing("mon-1").expect("trailing");
    assert!(!created);
    assert_eq!(m.active_id("mon-1").as_deref(), Some(id.as_str()));
    let ids = order_ids(&m, "mon-1");
    for (i, ws) in ids.iter().enumerate() {
        assert!(m.assign(key(100 + i as u64), "mon-1", ws, false));
    }
    let (_, created) = m.select_trailing("mon-1").expect("append");
    assert!(created);
    assert_eq!(m.workspace_count("mon-1"), 3);
}

#[test]
fn invalid_assign_refuses_without_loss() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
    let w = key(11);
    assert!(m.assign(w.clone(), "mon-1", &ws1, false));
    assert!(!m.assign(key(12), "mon-1", "ws-missing", false));
    assert!(!m.assign(key(13), "mon-missing", "ws-1", false));
    assert_eq!(m.member_loc(&w).expect("loc").workspace, ws1);
    let entry = m
        .outputs
        .get("mon-1")
        .expect("o")
        .order
        .iter()
        .find(|e| e.id == ws1)
        .expect("e");
    assert!(entry.members.contains(&w));
}

#[test]
fn cleanup_preserves_active_id_across_earlier_prune() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    for i in 0..2 {
        let ids = order_ids(&m, "mon-1");
        for ws in &ids {
            let occupied = m
                .outputs
                .get("mon-1")
                .expect("o")
                .order
                .iter()
                .find(|e| &e.id == ws)
                .expect("e")
                .members
                .is_empty();
            if occupied {
                assert!(m.assign(key(500 + i), "mon-1", ws, false));
                break;
            }
        }
        let _ = m.select_trailing("mon-1");
    }
    let active = m.active_id("mon-1").expect("active");
    let (removed, append) = m.plan_cleanup("mon-1", std::slice::from_ref(&active), &[]);
    assert!(!removed.contains(&active));
    m.apply_cleanup("mon-1", &removed, append);
    assert_eq!(m.active_id("mon-1").as_deref(), Some(active.as_str()));
    assert!(m.workspace_count("mon-1") >= policy::MIN_WORKSPACES);
}

#[test]
fn activate_by_id_covers_trailing_past_ordinals() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    // Fill every workspace so trailing appends past the Win1..9 range.
    for i in 0..10 {
        let ids = order_ids(&m, "mon-1");
        for ws in &ids {
            let occupied = m
                .outputs
                .get("mon-1")
                .expect("o")
                .order
                .iter()
                .find(|e| &e.id == ws)
                .expect("e")
                .members
                .is_empty();
            if occupied {
                assert!(m.assign(key(900 + i), "mon-1", ws, false));
                break;
            }
        }
        let _ = m.select_trailing("mon-1");
    }
    assert!(m.workspace_count("mon-1") > 9);
    let last = order_ids(&m, "mon-1").last().cloned().expect("last");
    let first = order_ids(&m, "mon-1").first().cloned().expect("first");
    assert!(m.activate("mon-1", &last));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(last.as_str()));
    assert!(m.activate("mon-1", &first));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(first.as_str()));
    assert!(!m.activate("mon-1", "ws-missing"));
    assert!(!m.activate("mon-missing", &first));
}

#[test]
fn per_output_isolation() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    m.select("mon-1", 2);
    assert_eq!(m.outputs.get("mon-1").expect("a").active, 1);
    assert_eq!(m.outputs.get("mon-1").expect("a").active, 1);
    let a = m.resolve_send("mon-1", 1).expect("send target");
    let b = m.resolve_send("mon-2", 1).expect("send target");
    assert_ne!(a, b);
}

#[test]
fn hidden_members_retained_and_foreground_selects_hidden() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let target = m.resolve_send("mon-1", 2).expect("ws2");
    let w = key(42);
    assert!(m.assign(w.clone(), "mon-1", &target, false));
    m.set_hidden(&w, true);
    assert!(m.is_hidden(&w));
    assert_eq!(m.member_loc(&w).expect("loc").workspace, target);
    let selected = m.select_for_foreground(&w).expect("foreground select");
    assert_eq!(selected, ("mon-1".to_owned(), target.clone()));
    let v = key(43);
    assert!(m.assign(v.clone(), "mon-1", &target, false));
    assert_eq!(m.select_for_foreground(&v), None);
}

#[test]
fn focus_target_avoids_hidden_previous() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
    let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
    let a = key(1);
    let b = key(2);
    assert!(m.assign(a.clone(), "mon-1", &ws2, false));
    assert!(m.assign(b.clone(), "mon-1", &ws2, false));
    m.note_foreground(&a);
    m.set_hidden(&a, true);
    let visible: BTreeSet<WindowKey> = [b.clone()].into_iter().collect();
    assert_eq!(m.focus_target("mon-1", &ws2, &visible), Some(b));
    assert_eq!(m.focus_target("mon-1", &ws1, &BTreeSet::new()), None);
}

#[test]
fn send_follow_refuses_stale_and_wrong_target() {
    let mover = key(7);
    let other = key(8);
    let source: BTreeSet<WindowKey> = [other.clone()].into_iter().collect();
    let target: BTreeSet<WindowKey> = [mover.clone(), other.clone()].into_iter().collect();
    assert!(verify_send_follow(&mover, &source, &target));
    let source: BTreeSet<WindowKey> = [mover.clone()].into_iter().collect();
    assert!(!verify_send_follow(&mover, &source, &target));
    let target: BTreeSet<WindowKey> = [other].into_iter().collect();
    assert!(!verify_send_follow(&mover, &BTreeSet::new(), &target));
}

#[test]
fn cleanup_floor_keeps_minimum_two() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let active = m.active_id("mon-1").expect("active");
    // Both workspaces empty and invisible: the floor keeps both.
    let (removed, append) = m.plan_cleanup("mon-1", &[], &[]);
    m.apply_cleanup("mon-1", &removed, append);
    assert!(m.workspace_count("mon-1") >= policy::MIN_WORKSPACES);
    assert!(m.active_id("mon-1").is_some());
    let _ = active;
}

#[test]
fn hidden_members_retain_workspace_through_cleanup() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
    let w = key(77);
    assert!(m.assign(w.clone(), "mon-1", &ws2, false));
    m.set_hidden(&w, true);
    let active = m.active_id("mon-1").expect("active");
    // The hidden-occupied workspace is protected from pruning.
    let (removed, append) = m.plan_cleanup("mon-1", std::slice::from_ref(&active), &[]);
    assert!(!removed.contains(&ws2));
    m.apply_cleanup("mon-1", &removed, append);
    assert_eq!(m.member_loc(&w).expect("loc").workspace, ws2);
    assert!(m.is_hidden(&w));
}

#[test]
fn last_focus_is_per_workspace() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
    let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
    let a = key(101);
    let b = key(102);
    assert!(m.assign(a.clone(), "mon-1", &ws1, false));
    assert!(m.assign(b.clone(), "mon-1", &ws2, false));
    m.note_foreground(&a);
    m.note_foreground(&b);
    let visible_a: BTreeSet<WindowKey> = [a.clone()].into_iter().collect();
    let visible_b: BTreeSet<WindowKey> = [b.clone()].into_iter().collect();
    assert_eq!(m.focus_target("mon-1", &ws1, &visible_a), Some(a));
    assert_eq!(m.focus_target("mon-1", &ws2, &visible_b), Some(b));
}

#[test]
fn remove_window_cleans_membership_and_focus() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
    let w = key(21);
    assert!(m.assign(w.clone(), "mon-1", &ws1, false));
    m.note_foreground(&w);
    assert!(m.remove_window(&w));
    assert!(m.member_loc(&w).is_none());
    assert!(!m.remove_window(&w));
}

#[test]
fn displaced_units_never_merge_and_return_without_baseline() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let before = m.workspace_count("mon-2");
    let returning_len = m.workspace_count("mon-1");
    let w = key(99);
    let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
    assert!(m.assign(w, "mon-1", &ws2, false));
    assert!(m.displace_output_to("mon-1", "mon-2"));
    assert!(!m.outputs.contains_key("mon-1"));
    assert!(!m.displace_output_to("mon-1", "mon-2"));
    assert!(!m.displace_output_to("mon-2", "mon-2"));
    assert!(m.reconnect_output("mon-1"));
    assert_eq!(
        m.workspace_count("mon-1"),
        returning_len,
        "no baseline duplicates"
    );
    assert_eq!(m.workspace_count("mon-2"), before);
    assert!(m.active_id("mon-1").is_some());
    assert!(!m.reconnect_output("mon-1-replaced"));
}

#[test]
fn workspace_event_is_opaque() {
    let ev = workspace_event("o1", "ws2", 3, "select", "ok");
    assert_eq!(ev["output"], serde_json::Value::from("o1"));
    assert_eq!(ev["members"], serde_json::Value::from(3));
    let text = ev.to_string();
    assert!(!text.contains("hwnd") && !text.contains("pid"));
}

#[test]
fn select_resolution_does_not_preactivate() {
    // Regression for `workspace_tick` mutating ACTIVE via
    // `select`/`select_trailing` before `workspace_do_select` reads
    // current (which then reports `already-active` with no native
    // effects). Select targets must resolve without activating; only the
    // transition itself activates after hide/reveal.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let before = m.active_id("mon-1").expect("active");
    let target = m.resolve_send("mon-1", 2).expect("resolve");
    assert_ne!(before, target);
    assert_eq!(
        m.active_id("mon-1").as_deref(),
        Some(before.as_str()),
        "resolve_send must not preactivate"
    );
    let before = m.active_id("mon-1").expect("active");
    let (trailing, _) = m.resolve_send_trailing("mon-1").expect("trailing");
    assert_eq!(
        m.active_id("mon-1").as_deref(),
        Some(before.as_str()),
        "resolve_send_trailing must not preactivate"
    );
    assert!(m.activate("mon-1", &trailing));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(trailing.as_str()));
}

#[test]
fn eligible_focus_uses_only_fresh_observation() {
    // Post-reveal focus must use the fresh eligible observation, never the
    // pre-switch membership alone: hidden and frameless members without a
    // fresh token never take focus, and the last appropriate fresh member
    // wins.
    use std::collections::{BTreeMap, HashSet};
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
    let a = key(1);
    let b = key(2);
    assert!(m.assign(a.clone(), "mon-1", &ws2, false));
    assert!(m.assign(b.clone(), "mon-1", &ws2, false));
    m.note_foreground(&a);
    m.set_hidden(&a, true);
    let mut member_tokens: BTreeMap<WindowKey, String> = BTreeMap::new();
    member_tokens.insert(a.clone(), "tok-a".to_owned());
    member_tokens.insert(b.clone(), "tok-b".to_owned());
    // Only `b` is freshly observed eligible; `a` stays hidden.
    let fresh: HashSet<String> = ["tok-b".to_owned()].into_iter().collect();
    let members: BTreeSet<WindowKey> = [a.clone(), b.clone()].into_iter().collect();
    let eligible = m.eligible_focus_set(&members, &member_tokens, &fresh);
    assert_eq!(eligible, [b.clone()].into_iter().collect());
    assert_eq!(m.focus_target("mon-1", &ws2, &eligible), Some(b));
    // Empty fresh observation means an empty target: no focus.
    let empty = m.eligible_focus_set(&members, &member_tokens, &HashSet::new());
    assert!(empty.is_empty());
    assert_eq!(m.focus_target("mon-1", &ws2, &empty), None);
}

#[test]
fn eligible_focus_includes_retained_maximized_token() {
    // A maximized mover is retained, never eligible-observed: the caller
    // unions its verified retained token into the fresh focus set (focus
    // only; geometry still excludes it), so the follow/return focus
    // target resolves to the maximized member with exact foreground proof.
    use std::collections::{BTreeMap, HashSet};
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
    let maxed = key(1);
    let sibling = key(2);
    assert!(m.assign(maxed.clone(), "mon-1", &ws2, false));
    assert!(m.assign(sibling.clone(), "mon-1", &ws2, false));
    m.note_foreground(&maxed);
    let mut member_tokens: BTreeMap<WindowKey, String> = BTreeMap::new();
    member_tokens.insert(maxed.clone(), "tok-max".to_owned());
    member_tokens.insert(sibling.clone(), "tok-sib".to_owned());
    let members: BTreeSet<WindowKey> = [maxed.clone(), sibling.clone()].into_iter().collect();
    // Eligible-only set misses the retained maximized member.
    let eligible_only: HashSet<String> = ["tok-sib".to_owned()].into_iter().collect();
    let eligible = m.eligible_focus_set(&members, &member_tokens, &eligible_only);
    assert_eq!(eligible, [sibling.clone()].into_iter().collect());
    // Retained-inclusive focus set (production union) keeps both; the
    // last-focus maximized mover wins the follow target.
    let focus_fresh: HashSet<String> = ["tok-sib".to_owned(), "tok-max".to_owned()]
        .into_iter()
        .collect();
    let focus_eligible = m.eligible_focus_set(&members, &member_tokens, &focus_fresh);
    assert_eq!(
        focus_eligible,
        [maxed.clone(), sibling.clone()].into_iter().collect()
    );
    assert_eq!(m.focus_target("mon-1", &ws2, &focus_eligible), Some(maxed));
}

#[test]
fn sticky_only_intermediate_workspace_prunes_but_keeps_membership() {
    // Sticky members never occupy: a sticky-only intermediate workspace is
    // reported removed like an empty one, while the sticky membership
    // itself survives in the owner tables (retained float, rehomed by the
    // next sticky-off, dropped only by the member-drop path).
    use std::collections::BTreeSet;
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let first = m.active_id("mon-1").expect("active");
    let second = m.resolve_send("mon-1", 2).expect("second");
    // Occupy the trailing slot so a third workspace appends, leaving the
    // sticky-only second workspace intermediate (prunable, not floored).
    let normal_win = key(42);
    assert!(m.assign(normal_win.clone(), "mon-1", &second, false));
    let (third, _) = m.select_trailing("mon-1").expect("trailing");
    assert!(m.assign(normal_win.clone(), "mon-1", &third, false));
    let sticky_win = key(41);
    assert!(m.assign(sticky_win.clone(), "mon-1", &second, false));
    assert!(m.activate("mon-1", &first));
    let sticky: BTreeSet<WindowKey> = [sticky_win.clone()].into_iter().collect();
    // Without exclusion the intermediate workspace looks occupied and
    // survives; with sticky exclusion it is reported removed.
    let (kept, _) = m.plan_cleanup("mon-1", std::slice::from_ref(&first), &[]);
    assert!(
        !kept.contains(&second),
        "occupied intermediate survives, got {kept:?}"
    );
    let (removed, append) =
        m.plan_cleanup_excluding("mon-1", std::slice::from_ref(&first), &[], &sticky);
    assert_eq!(removed, vec![second.clone()], "sticky-only prunes");
    m.apply_cleanup("mon-1", &removed, append);
    assert_eq!(m.active_id("mon-1").as_deref(), Some(first.as_str()));
    // Membership is untouched by the plan: the owner retains the sticky
    // float for rehome on the next sticky-off.
    assert_eq!(
        m.member_loc(&sticky_win).map(|l| l.workspace.clone()),
        Some(second.clone())
    );
}

#[test]
fn plan_cleanup_excluding_matches_plain_without_sticky() {
    // No sticky members: excluding degenerates to the plain plan.
    use std::collections::BTreeSet;
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let active = m.active_id("mon-1").expect("active");
    let plain = m.plan_cleanup("mon-1", std::slice::from_ref(&active), &[]);
    let excluded = m.plan_cleanup_excluding(
        "mon-1",
        std::slice::from_ref(&active),
        &[],
        &BTreeSet::new(),
    );
    assert_eq!(plain, excluded);
}

#[test]
fn workspace_mode_defaults_tiled_and_toggles_per_workspace() {
    let mut m = ManagedWorkspaces::new();
    assert!(m.default_tiled());
    m.ensure_output("mon-1");
    let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
    let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
    assert!(m.is_tiled("mon-1", &ws1));
    assert!(m.is_tiled("mon-1", &ws2));
    assert!(m.set_tiled("mon-1", &ws1, false));
    assert!(!m.is_tiled("mon-1", &ws1));
    assert!(m.is_tiled("mon-1", &ws2));
    assert!(!m.set_tiled("mon-1", "ws-missing", false));
    assert!(!m.set_tiled("mon-missing", &ws1, false));
}

#[test]
fn live_default_applies_only_to_future_workspaces() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
    assert!(m.set_default_tiled(false));
    assert!(!m.set_default_tiled(false));
    // Existing workspaces keep their session state.
    assert!(m.is_tiled("mon-1", &ws1));
    // New workspaces seed from the live default.
    m.ensure_output("mon-2");
    let other = m.resolve_send("mon-2", 1).expect("other");
    assert!(!m.is_tiled("mon-2", &other));
    // Unknown workspaces read as the current default.
    assert!(!m.is_tiled("mon-2", "ws-missing"));
}

#[test]
fn current_tiled_is_none_without_scope() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    assert_eq!(m.current_tiled("mon-1"), Some(true));
    assert_eq!(m.current_tiled("mon-missing"), None);
    let active = m.active_id("mon-1").expect("active");
    assert!(m.set_tiled("mon-1", &active, false));
    assert_eq!(m.current_tiled("mon-1"), Some(false));
}

#[test]
fn cleanup_prunes_mode_state_with_workspaces() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let first = m.active_id("mon-1").expect("active");
    let second = m.resolve_send("mon-1", 2).expect("second");
    assert!(m.set_tiled("mon-1", &second, false));
    // An empty intermediate workspace prunes while survivors keep their
    // own session state; the pruned floating state leaves with it.
    assert!(m.assign(key(5), "mon-1", &first, false));
    assert!(m.assign(key(6), "mon-1", &second, false));
    let (third, created) = m.select_trailing("mon-1").expect("trailing");
    assert!(created);
    assert!(m.assign(key(6), "mon-1", &third, false));
    assert!(m.activate("mon-1", &first));
    let sticky: BTreeSet<WindowKey> = BTreeSet::new();
    let (removed, append) =
        m.plan_cleanup_excluding("mon-1", std::slice::from_ref(&first), &[], &sticky);
    assert!(removed.contains(&second));
    m.apply_cleanup("mon-1", &removed, append);
    assert!(!m.tiled.contains_key(&second));
    assert!(m.is_tiled("mon-1", &first));
}

#[test]
fn floating_workspaces_keep_project_membership_for_new_windows() {
    // New windows admitted to a floating workspace join project
    // membership (hide/reveal scope) with frames untouched: mode never
    // gates assignment, only geometry/effects.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
    assert!(m.set_tiled("mon-1", &ws1, false));
    let w = key(31);
    assert!(m.assign(w.clone(), "mon-1", &ws1, false));
    assert_eq!(m.member_loc(&w).expect("loc").workspace, ws1);
    assert!(!m.is_tiled("mon-1", &ws1));
    assert!(!m.is_hidden(&w));
}

#[test]
fn pending_releases_keep_latest_intent_per_domain() {
    // A failed float release queued as non-retile is superseded by the
    // later retile intent for the same domain: exactly one entry rides,
    // so a retry can never fire the stale float at a retiled layout.
    let mut pending = PendingReleases::new();
    assert!(pending.is_empty());
    pending.queue("mon-1", "ws-1", false);
    pending.queue("mon-1", "ws-1", true);
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending.snapshot(),
        vec![("mon-1".to_owned(), "ws-1".to_owned(), true)]
    );
    // A confirmed toggle clears its domain only.
    pending.queue("mon-1", "ws-2", false);
    assert!(pending.confirm("mon-1", "ws-1"));
    assert!(!pending.confirm("mon-1", "ws-1"));
    assert_eq!(
        pending.snapshot(),
        vec![("mon-1".to_owned(), "ws-2".to_owned(), false)]
    );
}

#[test]
fn pending_releases_go_stale_once_tiled() {
    // Both intents imply a floating workspace: a workspace reading tiled
    // again must never release. Unknown workspaces still release (empty
    // rows clean up the orphaned Engine session).
    assert!(PendingReleases::is_stale(Some(true)));
    assert!(!PendingReleases::is_stale(Some(false)));
    assert!(!PendingReleases::is_stale(None));
}

#[test]
fn topology_fingerprint_moves_only_with_topology() {
    let areas = vec![("mon-1".to_owned(), (0, 0, 800, 600), (0, 0, 800, 600))];
    let workspaces = vec![("mon-1".to_owned(), vec!["ws-1".to_owned()])];
    let members = vec![key(1)];
    let base = topology_fingerprint(&areas, &workspaces, &members, 1);
    assert_eq!(topology_fingerprint(&areas, &workspaces, &members, 1), base);
    // Member swap at the same count still moves: a same-count change is
    // a topology edge, never a quiet tick.
    assert_ne!(
        topology_fingerprint(&areas, &workspaces, &[key(2)], 1),
        base
    );
    assert_ne!(topology_fingerprint(&areas, &workspaces, &members, 2), base);
    let moved_areas = vec![("mon-1".to_owned(), (0, 0, 1024, 768), (0, 0, 1024, 768))];
    assert_ne!(
        topology_fingerprint(&moved_areas, &workspaces, &members, 1),
        base
    );
}

// Item 1 history/ring regression: observed changes record, failed
// attempts and same-view activation never do, and the ring wraps the
// full scoped order including trailing empty and ordinals beyond 9.

fn history_ids(m: &ManagedWorkspaces, output: &str) -> Vec<String> {
    m.workspace_ids(output)
}

fn occupy_all_then_trailing(m: &mut ManagedWorkspaces, output: &str, count: usize) {
    for i in 0..count {
        let ids = history_ids(m, output);
        let mut placed = false;
        for ws in &ids {
            let occupied = m.workspace_members(output, ws).is_empty();
            if occupied {
                assert!(m.assign(key(700 + i as u64), output, ws, false));
                placed = true;
                break;
            }
        }
        assert!(placed, "workspace available for {i}");
        let _ = m.select_trailing(output);
    }
}

#[test]
fn history_records_only_observed_changes() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ids = history_ids(&m, "mon-1");
    let (ws1, ws2) = (ids[0].clone(), ids[1].clone());
    // Fresh output primes the baseline with no previous: toggle is a
    // no-op until the next recorded change, and setters never record.
    assert_eq!(m.history_baseline("mon-1").as_deref(), Some(ws1.as_str()));
    assert_eq!(m.history_previous("mon-1"), None);
    assert_eq!(m.resolve_previous("mon-1"), None);
    assert!(m.activate("mon-1", &ws1));
    assert!(!m.observe_workspace_change("mon-1", &ws1));
    assert_eq!(m.history_previous("mon-1"), None);
    // Unknown output/current never records.
    assert!(!m.observe_workspace_change("mon-gone", &ws1));
    assert!(!m.observe_workspace_change("mon-1", "ws-gone"));
    // First observed change records previous=ws1.
    assert!(m.activate("mon-1", &ws2));
    assert!(m.observe_workspace_change("mon-1", &ws2));
    assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws1.as_str()));
    assert_eq!(m.resolve_previous("mon-1").as_deref(), Some(ws1.as_str()));
}

#[test]
fn history_repeated_toggle_walks_two_views() {
    // WS1->WS2->WS3->WS2->WS3: toggle alternates the last two observed
    // views (two-view toggle, not MRU traversal).
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ids = history_ids(&m, "mon-1");
    assert!(m.assign(key(701), "mon-1", &ids[0], false));
    assert!(m.assign(key(702), "mon-1", &ids[1], false));
    let (third, created) = m.select_trailing("mon-1").expect("third");
    assert!(created);
    // Trailing creation is itself an observed view change in production
    // (`workspace_do_select` records the verified transition).
    assert!(m.observe_workspace_change("mon-1", &third));
    assert!(m.activate("mon-1", &ids[0]));
    assert!(m.observe_workspace_change("mon-1", &ids[0]));
    let ids = history_ids(&m, "mon-1");
    let (ws1, ws2, ws3) = (ids[0].clone(), ids[1].clone(), third.clone());
    for target in [&ws2, &ws3, &ws2, &ws3] {
        assert!(m.activate("mon-1", target));
        assert!(m.observe_workspace_change("mon-1", target));
    }
    assert_eq!(m.history_baseline("mon-1").as_deref(), Some(ws3.as_str()));
    assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws2.as_str()));
    // Toggle target resolves to ws2 without activating.
    let before = m.active_id("mon-1").expect("active");
    assert_eq!(m.resolve_previous("mon-1").as_deref(), Some(ws2.as_str()));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(before.as_str()));
    assert!(m.activate("mon-1", &ws2));
    assert!(m.observe_workspace_change("mon-1", &ws2));
    assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws3.as_str()));
    let _ = ws1;
}

#[test]
fn history_same_view_and_output_focus_never_record() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let ids = history_ids(&m, "mon-1");
    assert!(m.activate("mon-1", &ids[1]));
    assert!(m.observe_workspace_change("mon-1", &ids[1]));
    // Re-observing the same view is a no-op: previous stays.
    assert!(!m.observe_workspace_change("mon-1", &ids[1]));
    assert_eq!(
        m.history_previous("mon-1").as_deref(),
        Some(ids[0].as_str())
    );
    // Merely focusing another output (no view change there) records
    // nothing on either output.
    assert!(!m.observe_workspace_change("mon-2", &history_ids(&m, "mon-2")[0]));
    assert_eq!(m.history_previous("mon-2"), None);
}

#[test]
fn history_surviving_empty_stays_valid_but_removal_clears() {
    // Genuine lifecycle: visit ws2 (previous), empty it without removing
    // (stable id survives and still toggles), then prune it as an
    // intermediate empty through the real trailing plan. Removal clears
    // the previous entry: toggle is a no-op with no recreation.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ids = history_ids(&m, "mon-1");
    let (ws1, ws2) = (ids[0].clone(), ids[1].clone());
    // Occupy both so trailing appends a third workspace.
    let first_win = key(11);
    let second_win = key(12);
    assert!(m.assign(first_win.clone(), "mon-1", &ws1, false));
    assert!(m.assign(second_win.clone(), "mon-1", &ws2, false));
    let (ws3, created) = m.select_trailing("mon-1").expect("third");
    assert!(created);
    // Visit ws2, then return to ws1: previous is the visited ws2.
    assert!(m.activate("mon-1", &ws2));
    assert!(m.observe_workspace_change("mon-1", &ws2));
    assert!(m.activate("mon-1", &ws1));
    assert!(m.observe_workspace_change("mon-1", &ws1));
    assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws2.as_str()));
    // Empty the previous workspace without removing it: the stable id
    // survives emptiness and still resolves.
    assert!(m.remove_window(&second_win));
    assert!(m.workspace_members("mon-1", &ws2).is_empty());
    assert_eq!(m.resolve_previous("mon-1").as_deref(), Some(ws2.as_str()));
    // Order is now [ws1 occupied, ws2 empty intermediate, ws3 trailing
    // empty] with ws1 active: the real plan prunes exactly ws2.
    let (removed, append) = m.plan_cleanup("mon-1", std::slice::from_ref(&ws1), &[]);
    assert_eq!(removed, vec![ws2.clone()]);
    assert!(!append);
    m.apply_cleanup("mon-1", &removed, append);
    // Inventory is stable with no recreation: survivors keep their ids,
    // the active view is untouched, and the removed previous clears.
    assert_eq!(m.workspace_ids("mon-1"), vec![ws1.clone(), ws3.clone()]);
    assert_eq!(m.active_id("mon-1").as_deref(), Some(ws1.as_str()));
    assert_eq!(m.history_previous("mon-1"), None);
    assert_eq!(m.resolve_previous("mon-1"), None);
    let _ = first_win;
}

#[test]
fn history_disconnect_discards_and_reconnect_primes_fresh() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let ids = history_ids(&m, "mon-1");
    assert!(m.activate("mon-1", &ids[1]));
    assert!(m.observe_workspace_change("mon-1", &ids[1]));
    assert!(m.history_previous("mon-1").is_some());
    // Disconnect drops BOTH previous and baseline for the origin.
    assert!(m.displace_output_to("mon-1", "mon-2"));
    assert_eq!(m.history_previous("mon-1"), None);
    assert_eq!(m.history_baseline("mon-1"), None);
    // Reconnect primes a fresh baseline with no previous: toggle is a
    // no-op until the next recorded change, and selection never
    // consulted history (active is the first returning workspace).
    assert!(m.reconnect_output("mon-1"));
    assert_eq!(m.history_previous("mon-1"), None);
    let active = m.active_id("mon-1").expect("active");
    assert_eq!(
        m.history_baseline("mon-1").as_deref(),
        Some(active.as_str())
    );
    assert_eq!(m.resolve_previous("mon-1"), None);
    // The next observed displacement/return change records from the
    // fresh baseline.
    let ids = history_ids(&m, "mon-1");
    let other = ids.iter().find(|id| *id != &active).expect("other").clone();
    assert!(m.activate("mon-1", &other));
    assert!(m.observe_workspace_change("mon-1", &other));
    assert_eq!(
        m.history_previous("mon-1").as_deref(),
        Some(active.as_str())
    );
}

#[test]
fn ring_wraps_trailing_and_beyond_nine_without_creation() {
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    occupy_all_then_trailing(&mut m, "mon-1", 10);
    assert!(m.workspace_count("mon-1") > 9);
    let ring = m.scoped_ring_ids("mon-1");
    assert_eq!(ring, history_ids(&m, "mon-1"));
    let first = ring.first().expect("first").clone();
    let last = ring.last().expect("last").clone();
    // Last -> next wraps to first; first -> previous wraps to last.
    assert!(m.activate("mon-1", &last));
    assert_eq!(
        m.resolve_relative("mon-1", 1).as_deref(),
        Some(first.as_str())
    );
    assert!(m.activate("mon-1", &first));
    assert_eq!(
        m.resolve_relative("mon-1", -1).as_deref(),
        Some(last.as_str())
    );
    // Resolution creates nothing and refuses bad deltas.
    let count = m.workspace_count("mon-1");
    assert_eq!(m.resolve_relative("mon-1", 0), None);
    assert_eq!(m.resolve_relative("mon-1", 2), None);
    assert_eq!(m.workspace_count("mon-1"), count);
    // Unknown output refuses.
    assert_eq!(m.resolve_relative("mon-gone", 1), None);
}

#[test]
fn history_per_output_isolation_and_shared_scope() {
    // Local/global-unique model: per-output histories never leak.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let a = history_ids(&m, "mon-1");
    let b = history_ids(&m, "mon-2");
    assert!(m.activate("mon-1", &a[1]));
    assert!(m.observe_workspace_change("mon-1", &a[1]));
    assert_eq!(m.history_previous("mon-1").as_deref(), Some(a[0].as_str()));
    assert_eq!(m.history_previous("mon-2"), None);
    // Output focus alone on mon-2 records nothing.
    assert!(!m.observe_workspace_change("mon-2", &b[0]));
    // Shared model (future non-local runtime): one history/ring over the
    // full order, same record/no-op/invalidate rules, pure only.
    let live = vec!["s1".to_owned(), "s2".to_owned(), "s3".to_owned()];
    assert!(!m.observe_shared_change(&live, "s1"));
    assert!(m.resolve_shared_previous(&live).is_none());
    assert!(m.observe_shared_change(&live, "s2"));
    assert_eq!(m.resolve_shared_previous(&live).as_deref(), Some("s1"));
    assert_eq!(
        ManagedWorkspaces::resolve_shared_relative(&live, "s3", 1).as_deref(),
        Some("s1")
    );
    assert_eq!(
        ManagedWorkspaces::resolve_shared_relative(&live, "s1", -1).as_deref(),
        Some("s3")
    );
    // Removed shared ids clear through the genuine path, never
    // reinterpret: record s2 as previous against the full ring, then
    // resolve against a ring without it.
    let short = vec!["s1".to_owned(), "s3".to_owned()];
    assert_eq!(m.resolve_shared_previous(&short).as_deref(), Some("s1"));
    assert!(m.observe_shared_change(&live, "s3"));
    assert_eq!(m.resolve_shared_previous(&live).as_deref(), Some("s2"));
    assert_eq!(m.resolve_shared_previous(&short), None);
    assert_eq!(m.shared_previous, None);
}

#[test]
fn history_moved_previous_clears_like_removal() {
    // A previous entry that moves to another output's scope clears on
    // the recording output (return-on-reconnect case): toggle is a
    // no-op until the next recorded change.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let ids = history_ids(&m, "mon-1");
    assert!(m.activate("mon-1", &ids[1]));
    assert!(m.observe_workspace_change("mon-1", &ids[1]));
    assert!(m.displace_output_to("mon-1", "mon-2"));
    assert!(m.reconnect_output("mon-1"));
    assert_eq!(m.history_previous("mon-1"), None);
    // Unchanged inventory: displacement/return moved workspaces without
    // creating or duplicating ids on the survivor.
    let survivor = history_ids(&m, "mon-2");
    let mut seen = survivor.clone();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), survivor.len());
}

fn migration_fixture() -> (ManagedWorkspaces, Vec<String>, Vec<String>) {
    // Two outputs, three workspaces on the source: ws1 occupied, ws2
    // active with two members and remembered focus, ws3 trailing empty.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let seed = history_ids(&m, "mon-1");
    let dst = history_ids(&m, "mon-2");
    assert!(m.assign(key(21), "mon-1", &seed[0], false));
    let mover_a = key(22);
    let mover_b = key(23);
    assert!(m.assign(mover_a.clone(), "mon-1", &seed[1], false));
    assert!(m.assign(mover_b.clone(), "mon-1", &seed[1], true));
    m.note_foreground(&mover_a);
    // Occupy the trailing empty so the next spare appends a third
    // workspace; the move then migrates the middle entry.
    let (third, created) = m.select_trailing("mon-1").expect("third");
    assert!(created);
    assert!(m.activate("mon-1", &seed[1]));
    let src = history_ids(&m, "mon-1");
    assert_eq!(src.len(), 3);
    assert_eq!(src[2], third);
    (m, src, dst)
}

#[test]
fn migrate_preserves_state_and_inserts_after_target_current() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, dst) = migration_fixture();
    let (ws1, ws2) = (src[0].clone(), src[1].clone());
    // Distinct tiling state per workspace survives the move.
    assert!(m.set_tiled("mon-1", &ws2, false));
    assert!(m.set_tiled("mon-2", &dst[0], false));
    // Target current is its second workspace; the moved entry lands
    // immediately after it and shows.
    assert!(m.activate("mon-2", &dst[1]));
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
        .expect("migrate");
    assert_eq!(outcome.moved_id, ws2);
    assert_eq!(outcome.source_output, "mon-1");
    assert_eq!(outcome.target_output, "mon-2");
    // Destination order: [dst0, dst1, ws2-after-current]; moved shows.
    assert_eq!(
        m.workspace_ids("mon-2"),
        vec![dst[0].clone(), dst[1].clone(), ws2.clone()]
    );
    assert_eq!(m.active_id("mon-2").as_deref(), Some(ws2.as_str()));
    // The previous target workspace stays listed (hidden, not removed).
    assert!(m.workspace_ids("mon-2").contains(&dst[1]));
    // Stable identity: members, remembered focus and tiling state travel
    // with the id; nothing is re-admitted.
    assert_eq!(
        m.workspace_members("mon-2", &ws2),
        [key(22), key(23)].into_iter().collect()
    );
    let moved_entry = m
        .outputs
        .get("mon-2")
        .expect("target")
        .order
        .iter()
        .find(|e| e.id == ws2)
        .expect("moved entry");
    assert_eq!(moved_entry.last_focus, Some(key(22)));
    assert_eq!(m.member_loc(&key(22)).expect("loc").output, "mon-2");
    assert!(m.member_loc(&key(23)).expect("loc").hidden);
    assert!(!m.is_tiled("mon-2", &ws2));
    assert!(!m.is_tiled("mon-2", &dst[0]));
    // Source keeps its remaining workspaces with no recreation.
    assert_eq!(m.workspace_ids("mon-1"), vec![ws1.clone(), src[2].clone()]);
    let _ = ws1;
}

#[test]
fn migrate_default_refill_selects_last_remaining() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, _) = migration_fixture();
    // No recorded history: the default refills the last remaining scoped
    // entry (the trailing empty here).
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
        .expect("migrate");
    assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(src[2].as_str()));
}

#[test]
fn migrate_mru_prefers_eligible_previous_over_trailing() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, _) = migration_fixture();
    let (ws1, ws2) = (src[0].clone(), src[1].clone());
    // The fixture shows ws2 without an observation yet: observing it
    // records ws1 as the previous view (baseline was the initial ws1).
    assert!(m.observe_workspace_change("mon-1", &ws2));
    assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws1.as_str()));
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
        .expect("migrate");
    assert_eq!(outcome.source_refill.as_deref(), Some(ws1.as_str()));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(ws1.as_str()));
}

#[test]
fn migrate_mru_falls_back_without_eligible_previous() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, _) = migration_fixture();
    // Case 1: no history at all falls back to last remaining.
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
        .expect("migrate");
    assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
    // Case 2: a previous entry that left scope falls
    // back instead of recreating.
    let (mut m, src, _) = migration_fixture();
    m.previous.insert("mon-1".to_owned(), "ws-gone".to_owned());
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
        .expect("migrate");
    assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
    assert_eq!(m.workspace_ids("mon-1").len(), 2);
}

#[test]
fn migrate_mru_accepts_surviving_empty_previous() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, _) = migration_fixture();
    // Visit the trailing empty, then return to ws2: the remembered empty
    // survives in scope and refills under MRU.
    let empty = src[2].clone();
    let ws2 = src[1].clone();
    assert!(m.activate("mon-1", &empty));
    assert!(m.observe_workspace_change("mon-1", &empty));
    assert!(m.activate("mon-1", &ws2));
    assert!(m.observe_workspace_change("mon-1", &ws2));
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
        .expect("migrate");
    assert_eq!(outcome.source_refill.as_deref(), Some(empty.as_str()));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(empty.as_str()));
}

#[test]
fn migrate_uses_output_local_history_only() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, dst) = migration_fixture();
    // mon-2 records its own previous; the mon-1 migration must not
    // consult it (per-output scope, never global).
    assert!(m.activate("mon-2", &dst[1]));
    assert!(m.observe_workspace_change("mon-2", &dst[1]));
    assert_eq!(
        m.history_previous("mon-2").as_deref(),
        Some(dst[0].as_str())
    );
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
        .expect("migrate");
    assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
    assert!(m.history_previous("mon-2").is_some());
}

#[test]
fn migrate_empty_source_reports_no_refill_without_recreation() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let ids = history_ids(&m, "mon-1");
    // Trim the source to a single active workspace through the struct's
    // own order (test-only): migrating it leaves no remaining scope, so
    // both modes report no refill and the source keeps an empty order
    // for ordinary lifecycle to converge (nothing recreated here).
    {
        let state = m.outputs.get_mut("mon-1").expect("source");
        state.order.truncate(1);
        state.active = 0;
    }
    for refill in [Refill::LastRemaining, Refill::MostRecentlyUsed] {
        let mut m = m.clone();
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", refill)
            .expect("migrate");
        assert_eq!(outcome.moved_id, ids[0]);
        assert_eq!(outcome.source_refill, None);
        assert!(m.workspace_ids("mon-1").is_empty());
        assert!(m.workspace_ids("mon-2").contains(&ids[0]));
    }
}

#[test]
fn migrate_invalidates_moved_history_and_keeps_sibling_return() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, _) = migration_fixture();
    let (ws1, ws2) = (src[0].clone(), src[1].clone());
    // Previous points at the workspace about to move: after migration it
    // clears like removal (out of source scope).
    m.previous.insert("mon-1".to_owned(), ws2.clone());
    // Sibling hotplug-return association on another origin survives; the
    // moved id loses auto return.
    m.displaced.insert(
        "mon-gone".to_owned(),
        DisplacedRecord {
            workspace_ids: vec![ws1.clone(), "ws-other".to_owned()],
            dest_key: "mon-1".to_owned(),
        },
    );
    m.displaced.insert(
        "mon-old".to_owned(),
        DisplacedRecord {
            workspace_ids: vec![ws2.clone()],
            dest_key: "mon-2".to_owned(),
        },
    );
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
        .expect("migrate");
    assert_eq!(outcome.moved_id, ws2);
    assert_eq!(m.history_previous("mon-1"), None);
    let gone = m.displaced.get("mon-gone").expect("sibling record");
    assert_eq!(gone.workspace_ids, vec![ws1.clone(), "ws-other".to_owned()]);
    assert!(!m.displaced.contains_key("mon-old"));
    let _ = ws1;
}

#[test]
fn migrate_refuses_same_unknown_or_empty_source() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, _, _) = migration_fixture();
    let before = m.displaced_snapshot();
    assert!(
        m.migrate_workspace("mon-1", "mon-1", Refill::LastRemaining)
            .is_none()
    );
    assert!(
        m.migrate_workspace("mon-1", "mon-gone", Refill::LastRemaining)
            .is_none()
    );
    assert!(
        m.migrate_workspace("mon-gone", "mon-2", Refill::LastRemaining)
            .is_none()
    );
    // Refusals change nothing, including return associations.
    assert_eq!(m.displaced_snapshot(), before);
    assert_eq!(m.workspace_ids("mon-1").len(), 3);
}

#[test]
fn migrate_excluding_keeps_sticky_on_source_refill() {
    use tiler_core::output_selection::MigrationSourceRefill as Refill;
    let (mut m, src, _) = migration_fixture();
    let ws2 = src[1].clone();
    // The hidden member of the moved workspace is sticky: the filtered
    // primitive strips it from the moved entry and re-homes it to the
    // selected source refill with hidden state preserved.
    let sticky = key(23);
    let sticky_set: BTreeSet<WindowKey> = [sticky.clone()].into_iter().collect();
    let outcome = m
        .migrate_workspace_excluding("mon-1", "mon-2", Refill::LastRemaining, &sticky_set)
        .expect("migrate");
    assert_eq!(outcome.moved_id, ws2);
    let target_members = m.workspace_members("mon-2", &ws2);
    assert!(
        !target_members.contains(&sticky),
        "sticky never rides the moved entry"
    );
    assert!(
        target_members.contains(&key(22)),
        "migrating member travels"
    );
    let refill = outcome.source_refill.expect("refill");
    let loc = m.member_loc(&sticky).expect("sticky homed");
    assert_eq!(loc.output, "mon-1");
    assert_eq!(loc.workspace, refill);
    assert!(m.is_hidden(&sticky), "sticky hidden state preserved");
    assert!(m.workspace_members("mon-1", &refill).contains(&sticky));
    // The unfiltered wrapper keeps prior behavior (sticky travels with
    // the entry) so existing callers are unaffected.
    let (mut m, src, _) = migration_fixture();
    let ws2 = src[1].clone();
    m.migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
        .expect("migrate");
    assert!(m.workspace_members("mon-2", &ws2).contains(&key(23)));
}
