#![forbid(unsafe_code)]
// Windows best-effort admission settings and startup/live Engine policy:
// the catalog-mapped admission
// fields decode with largest-splittable/float defaults, refuse invalid
// tokens through the strict last-good path, adopt at startup and live for
// subsequent decisions only (no reflow/retry on Apply), retain topology
// and float intent across the Apply, and produce positive native-target
// plans under overlap. Offline only: no windows, hooks, or effects.
use super::rmax03_adapter_tests::test_state;
use super::{clear_pending_seams, inject_pending_tag};
use crate::settings::{ANCHOR_FALLBACK_NO_SEARCH, LiveSettings, NO_SPACE_OVERLAP, Settings};
use tiler_core::admission::{AnchorFallback, NoSpaceResult};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

/// Domain bounds with room for exactly one tile at gap 8: a lone leaf
/// projects, but any binary split needs a `>= gap + 2 = 10` long edge no
/// `9 x 9` tree offers on either axis. The second window exhausts
/// deterministically through ordinary anchor plus D2 fallback into the
/// D3 policy, independent of tree shape or split axis.
const TIGHT_BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 9,
    h: 9,
};
const GAP: i32 = 8;

fn member_key(hwnd: u64, pid: u32) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid,
        creation: format!("c{hwnd:016x}"),
    }
}

fn frame(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn stage_member(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    key: &crate::workspace::WindowKey,
    token: &str,
    tag: &str,
) {
    assert!(
        state
            .workspaces
            .assign(key.clone(), output, workspace, false)
    );
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
    state.member_tags.insert(key.clone(), tag.to_owned());
    inject_pending_tag(key.hwnd, Some(tag.to_owned()));
}

fn tiny_domain(
    output: &str,
    workspace: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    crate::workspace_owner::workspace_domain(output, workspace, TIGHT_BOUNDS, GAP)
}

fn committed_revision(state: &super::TileLoop, key: &tiler_core::session::DomainKey) -> u64 {
    state
        .engine
        .session(key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0)
}

/// Drive one Reconcile through the production adapter seam
/// (`handle_engine_event`: stamping plus commit) over explicit rows.
fn drive_reconcile(
    state: &mut super::TileLoop,
    domain: &tiler_core::session::OutputDomain,
    key: &tiler_core::session::DomainKey,
    rows: &[(&str, Rect, bool)],
    focused: Option<&str>,
) -> tiler_core::boundary::CoreReply {
    use tiler_core::boundary::CoreReply;
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = rows
        .iter()
        .map(|(token, rect, floating)| {
            (
                WindowId((*token).to_owned()),
                *rect,
                tiler_core::size_hints::WindowSizeHints::none(),
                *floating,
            )
        })
        .collect();
    let revision = committed_revision(state, key);
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(token, rect, _)| ((*token).to_owned(), *rect))
            .collect::<Vec<_>>(),
    );
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("admission-settings").expect("correlation"),
        revision,
        fp,
        domain,
        key,
        state.outer_gap,
        &carried,
        focused.map(|token| WindowId(token.to_owned())).as_ref(),
    );
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        !matches!(reply, CoreReply::Diverged(_)),
        "no divergence, got {reply:?}"
    );
    reply
}

#[test]
fn proof_owners_without_live_settings_keep_admission_defaults() {
    assert_eq!(
        super::admission::anchor_fallback_for(None),
        AnchorFallback::LargestSplittable
    );
    assert_eq!(super::admission::no_space_for(None), NoSpaceResult::Float);
}

#[test]
fn live_setting_selects_subsequent_admission_policy() {
    let live = LiveSettings::fresh(Settings::default(), None);
    assert_eq!(
        super::admission::anchor_fallback_for(Some(&live)),
        AnchorFallback::LargestSplittable
    );
    assert_eq!(
        super::admission::no_space_for(Some(&live)),
        NoSpaceResult::Float
    );
    let mut changed = Settings::default();
    changed.core.anchor_fallback = ANCHOR_FALLBACK_NO_SEARCH.to_owned();
    changed.core.no_space_result = NO_SPACE_OVERLAP.to_owned();
    let live = LiveSettings::fresh(changed, None);
    assert_eq!(
        super::admission::anchor_fallback_for(Some(&live)),
        AnchorFallback::NoSearch
    );
    assert_eq!(
        super::admission::no_space_for(Some(&live)),
        NoSpaceResult::AllowOverlap
    );
}

#[test]
fn apply_admission_only_returns_false_adopts_and_retains_topology_and_float_intent() {
    use crate::settings::{build_disabled, build_remap, tile_options_from_settings};
    use tiler_core::boundary::CoreReply;

    // Scratch home for the ledger lock and the owner log line: never the
    // repo or the live product directory. No window, hook, suspend, or
    // registry effect exists on this path; the temp dir is removed.
    let tmp = std::env::temp_dir().join(format!(
        "tiler-admission-live-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    clear_pending_seams();
    let mut state = test_state();
    state.log_path = tmp.join("owner.log");
    state.engine.set_fixed_size_admission(true);
    state.engine.set_fixed_send_admission(true);
    let base = tile_options_from_settings(&Settings::default());
    state.border = base.border;
    state.underlay = base.underlay;
    state.keyboard = crate::snapkey::KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    state.last_remap = build_remap(&Settings::default());
    state.last_disabled = build_disabled(&Settings::default());
    state.settings_live = Some(LiveSettings::fresh(Settings::default(), None));
    // Startup policy mirrors the owner entry: defaults before any decision.
    state
        .engine
        .set_anchor_fallback(super::admission::anchor_fallback_for(
            state.settings_live.as_ref(),
        ));
    state
        .engine
        .set_no_space_result(super::admission::no_space_for(state.settings_live.as_ref()));
    assert_eq!(
        state.engine.anchor_fallback(),
        AnchorFallback::LargestSplittable
    );
    assert_eq!(state.engine.no_space_result(), NoSpaceResult::Float);
    let mut snap_want = true;

    // Seed one tile plus one committed capacity float under the defaults.
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x701, 1701);
    let key_b = member_key(0x702, 1702);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_a = TIGHT_BOUNDS;
    let frame_b = frame(2, 2, 5, 5);
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false)],
        Some("wA"),
    );
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "first seed converges, got {reply:?}"
    );
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false), ("wB", frame_b, false)],
        Some("wA"),
    );
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "exhausted admission converges, got {reply:?}"
    );
    assert!(
        state
            .engine
            .session(&key)
            .expect("session")
            .is_capacity_float(&WindowId("wB".to_owned())),
        "float seed"
    );
    let before = state.engine.session(&key).expect("session").snapshot();

    // Apply a policy-only change: adopts for subsequent decisions with no
    // reflow, no retry, and no topology/float reclassification.
    let me = crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    };
    let mut incoming = Settings::default();
    incoming.core.anchor_fallback = ANCHOR_FALLBACK_NO_SEARCH.to_owned();
    incoming.core.no_space_result = NO_SPACE_OVERLAP.to_owned();
    incoming.revision = 7;
    let changed = super::apply_live_settings(
        &mut state,
        &tmp,
        &me,
        &store,
        &mut snap_want,
        incoming,
        None,
    );
    assert!(!changed, "policy-only adoption reports no owner change");
    assert_eq!(state.engine.anchor_fallback(), AnchorFallback::NoSearch);
    assert_eq!(state.engine.no_space_result(), NoSpaceResult::AllowOverlap);
    let session = state.engine.session(&key).expect("session");
    assert_eq!(
        session.snapshot(),
        before,
        "retained session untouched: no rebuild, no resync"
    );
    assert!(
        session.is_capacity_float(&WindowId("wB".to_owned())),
        "retained capacity float keeps its intent until the next decision"
    );
    assert!(
        !session.is_capacity_float(&WindowId("wA".to_owned())),
        "retained tile never reclassifies on reload"
    );
    assert_eq!((state.inner_gap, state.outer_gap), (8, 8));
    drop(store);
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn overlap_yields_positive_native_target_plans() {
    // Tight bounds under allow-overlap: the exhausted newcomer keeps tiled
    // membership with positive overflowing geometry instead of floating.
    // Every native-target rectangle stays strictly positive, so the write
    // path never emits a degenerate frame.
    clear_pending_seams();
    let mut state = test_state();
    state.engine.set_fixed_size_admission(true);
    state
        .engine
        .set_no_space_result(NoSpaceResult::AllowOverlap);
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x711, 1711);
    let key_b = member_key(0x712, 1712);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_a = TIGHT_BOUNDS;
    let frame_b = frame(2, 2, 5, 5);
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false), ("wB", frame_b, false)],
        Some("wB"),
    );
    let writes = crate::workspace_owner::planned_writes(&reply).unwrap_or_default();
    let tokens: Vec<String> = writes.iter().map(|write| write.window.0.clone()).collect();
    assert!(
        tokens.contains(&"wA".to_owned()) && tokens.contains(&"wB".to_owned()),
        "overlap keeps both tiled, got {tokens:?}"
    );
    for write in &writes {
        assert!(
            write.rect.w >= 1 && write.rect.h >= 1,
            "positive native target for {}: {:?}",
            write.window.0,
            write.rect
        );
    }
    let session = state.engine.session(&key).expect("session");
    assert!(
        !session.is_capacity_float(&WindowId("wB".to_owned())),
        "overlap newcomer is not a capacity float"
    );
    assert!(
        !session.is_exception(&WindowId("wB".to_owned())),
        "overlap newcomer holds a tile slot"
    );
    clear_pending_seams();
}

#[test]
fn next_decision_uses_adopted_overlap_after_float_seed() {
    // Next-decision timing (D10): a float seed under the default stays
    // floating after the Apply, while a later newcomer under the adopted
    // overlap policy tiles with positive geometry.
    use tiler_core::boundary::CoreReply;
    clear_pending_seams();
    let mut state = test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x721, 1721);
    let key_b = member_key(0x722, 1722);
    let key_c = member_key(0x723, 1723);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_a = TIGHT_BOUNDS;
    let frame_b = frame(2, 2, 5, 5);
    let frame_c = frame(1, 1, 4, 4);
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false)],
        Some("wA"),
    );
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "first seed converges, got {reply:?}"
    );
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false), ("wB", frame_b, false)],
        Some("wA"),
    );
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "float seed converges, got {reply:?}"
    );
    assert!(
        state
            .engine
            .session(&key)
            .expect("session")
            .is_capacity_float(&WindowId("wB".to_owned())),
        "seed floats under the default"
    );
    // Adopt overlap for subsequent decisions only: no reflow follows.
    state
        .engine
        .set_no_space_result(NoSpaceResult::AllowOverlap);
    let session = state.engine.session(&key).expect("session");
    assert!(
        session.is_capacity_float(&WindowId("wB".to_owned())),
        "seed float intent survives the policy change"
    );
    stage_member(&mut state, &output, &ws, &key_c, "wC", "tag-wc");
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[
            ("wA", frame_a, false),
            ("wB", frame_b, false),
            ("wC", frame_c, false),
        ],
        Some("wC"),
    );
    let writes = crate::workspace_owner::planned_writes(&reply).unwrap_or_default();
    for write in &writes {
        assert!(
            write.rect.w >= 1 && write.rect.h >= 1,
            "positive native target for {}: {:?}",
            write.window.0,
            write.rect
        );
    }
    clear_pending_seams();
}
