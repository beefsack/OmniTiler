#![forbid(unsafe_code)]
use tiler_core::directional::Direction;
use tiler_windows::snapkey::{
    Classified, INTENT_QUEUE_CAP, KeyboardConfig, MARKED_DIAG_CAP, MOD_DIAG_CAP, MarkedDiagBuf,
    MarkedKeyDiag, ModDiagBuf, ModSource, ModTrafficDiag, OriginVerdict, QueuedSnapEvent,
    SnapClassify, SnapEdge, SnapIntent, SnapOp, SnapOrigin, SnapQueue, VK_0, VK_CONTROL, VK_DOWN,
    VK_F11, VK_H, VK_J, VK_K, VK_L, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_M, VK_MASK, VK_MENU,
    VK_RIGHT, VK_RWIN, VK_SHIFT, VK_UP, WorkspaceOp, classify_and_queue, direction_name,
    is_chord_vk, is_fullscreen_vk, is_proof_mod_vk, is_win_vk, marked_diag_evidence,
    mod_diag_evidence, resolve_origin, stamp_mask_result, win_up_mask_reserve,
};
use tiler_windows::tiling::parse_tile_args;

fn takeover() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }
}

fn origin_of(hwnd: u64, token: &str) -> SnapOrigin {
    SnapOrigin {
        hwnd,
        token: token.to_owned(),
        pid: 1000 + hwnd as u32,
        creation: format!("creation-{hwnd}"),
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

fn win_down(m: &mut SnapClassify, win: u32) {
    assert_eq!(SnapClassify::push(m, win, false, true, false), None);
}

/// Test helper: the unified classifier returns `Classified`; directional
/// tests unwrap the snap half. Digits use `push_workspace` below.
fn push_snap(
    m: &mut SnapClassify,
    vk: u32,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<SnapIntent> {
    match SnapClassify::push(m, vk, is_up, fg, inj)? {
        Classified::Snap(intent) => Some(intent),
        Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected directional chord")
        }
    }
}

fn push_workspace(
    m: &mut SnapClassify,
    vk: u32,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::WorkspaceIntent> {
    match SnapClassify::push(m, vk, is_up, fg, inj)? {
        Classified::Workspace(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected workspace digit")
        }
    }
}

fn push_maximize(
    m: &mut SnapClassify,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::MaximizeIntent> {
    use tiler_windows::snapkey::VK_M;
    match SnapClassify::push(m, VK_M, is_up, fg, inj)? {
        Classified::Maximize(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected maximize chord")
        }
    }
}

fn push_fullscreen(
    m: &mut SnapClassify,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::FullscreenIntent> {
    match SnapClassify::push(m, VK_F11, is_up, fg, inj)? {
        Classified::Fullscreen(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected fullscreen chord")
        }
    }
}

fn push_float(
    m: &mut SnapClassify,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::FloatIntent> {
    use tiler_windows::snapkey::VK_G;
    match SnapClassify::push(m, VK_G, is_up, fg, inj)? {
        Classified::Float(intent) => Some(intent),
        // Shifted G routes to the sticky arm: the float arm did not fire.
        Classified::Sticky(_) => None,
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Orientation(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_)
        | Classified::Fullscreen(_) => {
            panic!("expected float chord")
        }
    }
}

fn push_sticky(
    m: &mut SnapClassify,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::StickyIntent> {
    use tiler_windows::snapkey::VK_G;
    match SnapClassify::push(m, VK_G, is_up, fg, inj)? {
        Classified::Sticky(intent) => Some(intent),
        // Unshifted G routes to the float arm: the sticky arm did not fire.
        Classified::Float(_) => None,
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Orientation(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_)
        | Classified::Fullscreen(_) => {
            panic!("expected sticky chord")
        }
    }
}

fn push_orientation(
    m: &mut SnapClassify,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::OrientationIntent> {
    use tiler_windows::snapkey::VK_O;
    match SnapClassify::push(m, VK_O, is_up, fg, inj)? {
        Classified::Orientation(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Resize(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Sticky(_) => {
            panic!("expected orientation chord")
        }
    }
}

fn push_workspace_tiling(
    m: &mut SnapClassify,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::WorkspaceTilingIntent> {
    use tiler_windows::snapkey::VK_Y;
    match SnapClassify::push(m, VK_Y, is_up, fg, inj)? {
        Classified::WorkspaceTiling(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Resize(_)
        | Classified::Launch(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_) => {
            panic!("expected workspace-tiling chord")
        }
    }
}

// Letter and arrow aliases share one catalog: unshifted focuses, Shift moves.
#[test]
fn exact_catalog_maps_all_sixteen_chords() {
    let cases: &[(u32, Direction)] = &[
        (VK_H, Direction::Left),
        (VK_J, Direction::Down),
        (VK_K, Direction::Up),
        (VK_LEFT, Direction::Left),
        (VK_DOWN, Direction::Down),
        (VK_UP, Direction::Up),
        (VK_RIGHT, Direction::Right),
    ];
    for (vk, direction) in cases {
        // Focus: Win+key.
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        let focus = push_snap(&mut m, *vk, false, true, false).expect("focus down");
        assert_eq!(focus.op, SnapOp::Focus);
        assert_eq!(focus.direction, *direction);
        assert_eq!(focus.edge, SnapEdge::Down);
        assert!(focus.consumed && focus.announce);
        let up = push_snap(&mut m, *vk, true, true, false).expect("focus up");
        assert_eq!(
            (up.op, up.edge, up.consumed),
            (SnapOp::Focus, SnapEdge::Up, true)
        );
        // Move: Win+Shift+key.
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_RWIN);
        push_snap(&mut m, VK_SHIFT, false, true, false);
        let mv = push_snap(&mut m, *vk, false, true, false).expect("move down");
        assert_eq!(mv.op, SnapOp::Move);
        assert_eq!(mv.direction, *direction);
        assert!(mv.consumed && mv.announce);
        // Releasing Shift before the key-up keeps the down-time op.
        push_snap(&mut m, VK_SHIFT, true, true, false);
        let up = push_snap(&mut m, *vk, true, true, false).expect("move up");
        assert_eq!((up.op, up.consumed), (SnapOp::Move, true));
    }
}

#[test]
fn win_lshift_arrow_right_classifies_move() {
    // Live regression anchor: a marked Win+Shift+Right stream reached the
    // owner as consumed focus/right (run 20261002-024604-32652, tick 15).
    // The classifier half of that path must resolve LSHIFT-held RIGHT to
    // Move; callback delivery is covered by proof-keys diagnostics.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_LSHIFT, false, true, false);
    assert!(m.tracked_modifiers().2, "lshift down must track shift");
    assert!(m.win_held(), "win must track held across shift");
    let down = push_snap(&mut m, VK_RIGHT, false, true, false).expect("move down");
    assert_eq!((down.op, down.direction), (SnapOp::Move, Direction::Right));
    assert!(down.consumed && down.announce);
    push_snap(&mut m, VK_LSHIFT, true, true, false);
    assert!(!m.tracked_modifiers().2, "lshift up must clear shift");
}

#[test]
fn marked_diag_buf_bounds_evict_oldest_and_counts_loss() {
    let mut buf = MarkedDiagBuf::new();
    assert!(buf.is_empty());
    let rec = |vk: u32| MarkedKeyDiag {
        vk,
        scan: 0x2A,
        is_up: false,
        win: true,
        shift_before: false,
        shift_after: true,
        ctrl: false,
        alt: false,
        async_shift: false,
        guard_disagree: false,
        consumed: false,
    };
    for vk in 0..(MARKED_DIAG_CAP as u32 + 5) {
        buf.push(rec(vk));
    }
    assert_eq!(buf.len(), MARKED_DIAG_CAP);
    assert_eq!(buf.dropped, 5);
    let drained = buf.drain();
    assert_eq!(drained.len(), MARKED_DIAG_CAP);
    assert_eq!(drained[0].vk, 5, "oldest records evict first");
    assert!(buf.is_empty());
    assert_eq!(buf.dropped, 5, "drain keeps the loss count");
}

#[test]
fn marked_diag_evidence_is_key_identity_only() {
    let diag = MarkedKeyDiag {
        vk: VK_LSHIFT,
        scan: 0x2A,
        is_up: false,
        win: true,
        shift_before: false,
        shift_after: true,
        ctrl: false,
        alt: false,
        async_shift: true,
        guard_disagree: false,
        consumed: false,
    };
    let ev = marked_diag_evidence(&diag);
    assert_eq!(ev["vk"], serde_json::Value::from(VK_LSHIFT));
    assert_eq!(ev["scan"], serde_json::Value::from(0x2Au32));
    assert_eq!(ev["edge"], serde_json::Value::from("down"));
    assert_eq!(ev["shift_before"], serde_json::Value::from(false));
    assert_eq!(ev["shift_after"], serde_json::Value::from(true));
    assert_eq!(ev["async_shift"], serde_json::Value::from(true));
    let text = ev.to_string();
    for banned in [
        "hwnd",
        "pid",
        "process_creation",
        "user_sid",
        "exe_path",
        "title",
        "token",
    ] {
        assert!(
            !text.contains(&format!("\"{banned}\"")),
            "diag evidence must never carry {banned}"
        );
    }
}

#[test]
fn proof_mod_vk_covers_only_win_and_modifier_families() {
    use tiler_windows::snapkey::{VK_LCONTROL, VK_RCONTROL, VK_RMENU, VK_RSHIFT};
    for vk in [
        VK_LWIN,
        VK_RWIN,
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
    ] {
        assert!(is_proof_mod_vk(vk), "modifier {vk} must be recordable");
    }
    for vk in [
        VK_H, VK_J, VK_K, VK_L, VK_LEFT, VK_DOWN, VK_UP, VK_RIGHT, VK_MASK, 65, 90, 27,
    ] {
        assert!(!is_proof_mod_vk(vk), "non-modifier {vk} must never record");
    }
}

#[test]
fn mod_diag_buf_bounds_evict_oldest_and_counts_loss() {
    let mut buf = ModDiagBuf::new();
    assert!(buf.is_empty());
    let rec = |vk: u32| ModTrafficDiag {
        vk,
        scan: 0x2A,
        is_up: false,
        source: ModSource::Physical,
        win_before: true,
        win_after: true,
        shift_before: false,
        shift_after: true,
        ctrl: false,
        alt: false,
    };
    for vk in 0..(MOD_DIAG_CAP as u32 + 5) {
        buf.push(rec(vk));
    }
    assert_eq!(buf.len(), MOD_DIAG_CAP);
    assert_eq!(buf.dropped, 5);
    let drained = buf.drain();
    assert_eq!(drained.len(), MOD_DIAG_CAP);
    assert_eq!(drained[0].vk, 5, "oldest records evict first");
    assert!(buf.is_empty());
    assert_eq!(buf.dropped, 5, "drain keeps the loss count");
}

#[test]
fn mod_diag_evidence_is_modifier_identity_only() {
    let diag = ModTrafficDiag {
        vk: VK_LSHIFT,
        scan: 0x2A,
        is_up: true,
        source: ModSource::InjectedFiltered,
        win_before: true,
        win_after: true,
        shift_before: true,
        shift_after: false,
        ctrl: false,
        alt: false,
    };
    let ev = mod_diag_evidence(&diag);
    assert_eq!(ev["vk"], serde_json::Value::from(VK_LSHIFT));
    assert_eq!(ev["scan"], serde_json::Value::from(0x2Au32));
    assert_eq!(ev["edge"], serde_json::Value::from("up"));
    assert_eq!(ev["source"], serde_json::Value::from("injected-filtered"));
    assert_eq!(ev["shift_before"], serde_json::Value::from(true));
    assert_eq!(ev["shift_after"], serde_json::Value::from(false));
    assert_eq!(
        ModSource::Physical.as_str(),
        "physical",
        "physical source must serialize distinctly"
    );
    let text = ev.to_string();
    for banned in [
        "hwnd",
        "pid",
        "process_creation",
        "user_sid",
        "exe_path",
        "title",
        "token",
    ] {
        assert!(
            !text.contains(&format!("\"{banned}\"")),
            "mod evidence must never carry {banned}"
        );
    }
}

#[test]
fn stray_shift_up_between_shift_down_and_chord_flips_move_to_focus() {
    // Mechanism anchor for the live move blocker: the classifier resolves
    // the op from its own tracked Shift at chord-down time. A Shift-up that
    // reaches the callback between the marked Shift-down and the chord-down
    // (e.g. unmarked/physical traffic the old proof-keys buffer could not
    // see) clears tracked Shift, so Win+Shift+Right classifies focus/right.
    // This test pins the mechanism; the live proof-mods buffer now captures
    // whether such traffic actually occurs.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_LSHIFT, false, true, false);
    assert!(m.tracked_modifiers().2);
    // Stray Shift-up from any source clears tracked Shift.
    push_snap(&mut m, VK_LSHIFT, true, true, false);
    assert!(!m.tracked_modifiers().2);
    let down = push_snap(&mut m, VK_RIGHT, false, true, false).expect("chord down");
    assert_eq!((down.op, down.direction), (SnapOp::Focus, Direction::Right));
}

#[test]
fn both_win_keys_count() {
    for win in [VK_LWIN, VK_RWIN] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, win);
        let down = push_snap(&mut m, VK_J, false, true, false).expect("down");
        assert_eq!(down.direction, Direction::Down);
        assert!(is_win_vk(win));
    }
    assert!(!is_win_vk(VK_LEFT));
}

#[test]
fn unshifted_win_l_needs_explicit_opt_in() {
    // Default: unshifted Win+L passes through untracked, paired up too.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_snap(&mut m, VK_L, false, true, false), None);
    assert_eq!(push_snap(&mut m, VK_L, true, true, false), None);
    assert_eq!((m.counts[3].down, m.counts[3].up), (0, 0));
    // Opt-in: Win+L focuses right like the catalog says.
    let mut m = SnapClassify::new(KeyboardConfig {
        takeover: true,
        allow_win_l: true,
    });
    win_down(&mut m, VK_LWIN);
    let down = push_snap(&mut m, VK_L, false, true, false).expect("win+l down");
    assert_eq!((down.op, down.direction), (SnapOp::Focus, Direction::Right));
    // Win+Shift+L stays approved without the opt-in.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_LSHIFT, false, true, false);
    let mv = push_snap(&mut m, VK_L, false, true, false).expect("win+shift+l down");
    assert_eq!((mv.op, mv.direction), (SnapOp::Move, Direction::Right));
}

#[test]
fn extra_modifiers_and_injected_pass_untracked() {
    // Alt selects the keyboard resize arm on catalog keys: Win+Alt+H consumes
    // as resize-out (dedicated intent, never a directional move). Alt on a
    // non-catalog key (Win+Alt+M) still passes through untracked with no hold.
    for mod_vk in [VK_MENU, VK_LMENU] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        match SnapClassify::push(&mut m, VK_H, false, true, false).expect("resize down") {
            Classified::Resize(intent) => {
                assert!(intent.consumed && intent.announce);
                assert_eq!(intent.mode, tiler_windows::snapkey::ResizeMode::Outwards);
                assert_eq!(intent.direction, Direction::Left);
            }
            other => panic!("expected resize-out, got {other:?}"),
        }
        match SnapClassify::push(&mut m, VK_H, true, true, false).expect("resize up") {
            Classified::Resize(intent) => assert!(intent.consumed && !intent.announce),
            other => panic!("expected resize up, got {other:?}"),
        }
        assert_eq!(
            (m.resize_counts[0].down, m.resize_counts[0].up),
            (1, 1),
            "resize arm carries its own counts, never the directional slot"
        );
        assert_eq!(
            (m.counts[0].down, m.counts[0].up, m.counts[0].consumed),
            (0, 0, 0)
        );
        push_snap(&mut m, mod_vk, true, true, false);
        // Non-catalog Alt chord still passes with no pin, so its paired up
        // passes too.
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        assert_eq!(SnapClassify::push(&mut m, VK_M, false, true, false), None);
        assert_eq!(SnapClassify::push(&mut m, VK_M, true, true, false), None);
        push_snap(&mut m, mod_vk, true, true, false);
    }
    // Ctrl is the item 1 history arm, not an extra modifier: Win+Ctrl+H
    // classifies as history-prev (consumed), with its pair closing consumed.
    {
        use tiler_windows::snapkey::VK_CONTROL;
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        SnapClassify::push(&mut m, VK_CONTROL, false, true, false);
        match SnapClassify::push(&mut m, VK_H, false, true, false).expect("history down") {
            Classified::WorkspaceHistory(intent) => {
                assert!(intent.consumed && intent.announce);
                assert_eq!(intent.op, tiler_windows::snapkey::WorkspaceHistoryOp::Prev);
            }
            other => panic!("expected history-prev, got {other:?}"),
        }
        match SnapClassify::push(&mut m, VK_H, true, true, false).expect("history up") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
            other => panic!("expected history up, got {other:?}"),
        }
        SnapClassify::push(&mut m, VK_CONTROL, true, true, false);
    }
    // Injected chords never classify, never arm the mask.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_snap(&mut m, VK_H, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut q,
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
    assert!(q.is_empty());
    // Unrelated keys and bare arrows (no Win) pass untracked.
    let mut m = SnapClassify::new(takeover());
    assert_eq!(push_snap(&mut m, 65, false, true, false), None);
    assert_eq!(push_snap(&mut m, VK_LEFT, false, true, false), None);
    assert_eq!(push_snap(&mut m, VK_LEFT, true, true, false), None);
}

#[test]
fn origin_pairing_repeats_and_background() {
    // Win released before the key-up still pairs by origin.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_H, false, true, false);
    push_snap(&mut m, VK_LWIN, true, true, false);
    let up = push_snap(&mut m, VK_H, true, true, false).expect("paired up");
    assert!(up.consumed);
    // Authentic: a consumed hold stays consumed across foreground changes.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_K, false, true, false);
    let rep = push_snap(&mut m, VK_K, false, true, false).expect("repeat");
    assert_eq!(
        (rep.edge, rep.consumed, rep.announce),
        (SnapEdge::Repeat, true, true)
    );
    let bg = push_snap(&mut m, VK_K, false, false, false).expect("repeat");
    assert_eq!((bg.edge, bg.consumed), (SnapEdge::Repeat, true));
    let up = push_snap(&mut m, VK_K, true, false, false).expect("paired up");
    assert!(up.consumed);
    // Unmanaged-origin holds consume at interception (owner rechecks fresh
    // identity before any action, never manipulates protected windows).
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_snap(&mut m, VK_J, false, false, false).expect("unmanaged down");
    assert!(down.consumed);
    let rep = push_snap(&mut m, VK_J, false, true, false).expect("repeat");
    assert!(rep.consumed);
    let up = push_snap(&mut m, VK_J, true, true, false).expect("paired up");
    assert!(up.consumed);
    assert_eq!(push_snap(&mut m, VK_J, true, true, false), None);
}

#[test]
fn takeover_off_passes_but_logs() {
    let mut m = SnapClassify::new(KeyboardConfig {
        takeover: false,
        allow_win_l: false,
    });
    win_down(&mut m, VK_LWIN);
    let down = push_snap(&mut m, VK_H, false, true, false).expect("logged");
    assert!(!down.consumed && !down.announce);
    let up = push_snap(&mut m, VK_H, true, true, false).expect("logged");
    assert!(!up.consumed);
    assert_eq!((m.counts[0].down, m.counts[0].passed), (1, 2));
}

#[test]
fn saturated_queue_swallows_and_counts_loss() {
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    push_snap(&mut m, VK_LWIN, false, true, false);
    let tick = std::time::Instant::now();
    let origin = Some(origin_of(11, "w1"));
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_H,
            false,
            origin.clone(),
            false,
            tick,
            true
        ),
        Some(true)
    );
    // The queued record carries the chord-time origin for the owner recheck.
    match q.pop_front().expect("intent") {
        QueuedSnapEvent::Intent(queued) => assert_eq!(queued.origin, origin),
        QueuedSnapEvent::Workspace(_)
        | QueuedSnapEvent::WorkspaceSend(_)
        | QueuedSnapEvent::WorkspaceOutput(_)
        | QueuedSnapEvent::WorkspaceMigrate(_)
        | QueuedSnapEvent::WorkspaceHistory(_)
        | QueuedSnapEvent::Maximize(_)
        | QueuedSnapEvent::Fullscreen(_)
        | QueuedSnapEvent::Float(_)
        | QueuedSnapEvent::Orientation(_)
        | QueuedSnapEvent::Sticky(_)
        | QueuedSnapEvent::Launch(_)
        | QueuedSnapEvent::WorkspaceTiling(_)
        | QueuedSnapEvent::Resize(_)
        | QueuedSnapEvent::Mask(_) => panic!("expected intent"),
    }
    while q.len() < INTENT_QUEUE_CAP {
        assert!(q.push(QueuedSnapEvent::Intent(
            tiler_windows::snapkey::QueuedIntent {
                op: SnapOp::Focus,
                direction: Direction::Left,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick,
            }
        )));
    }
    assert!(q.is_full());
    // Authentic saturation: still swallows, drops only queued evidence.
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, true, origin, false, tick, true),
        Some(true)
    );
    assert_eq!(q.dropped, 1);
    assert!(m.enabled, "saturation must not latch the machine off");
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, false, None, true, tick, true),
        None
    );
    assert_eq!(q.dropped, 1, "injected input never counts loss");
}

#[test]
fn saturated_queue_preserves_earlier_consumed_mask() {
    // A consumed chord arms the Start-menu mask; a later saturated chord
    // still swallows (dropping only evidence) and must not disarm the
    // earlier mask. A redundant E8 pair is safe, a naked Win Start is not.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    push_snap(&mut m, VK_LWIN, false, true, false);
    let tick = std::time::Instant::now();
    let origin = Some(origin_of(11, "w1"));
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_H,
            false,
            origin.clone(),
            false,
            tick,
            true
        ),
        Some(true)
    );
    while q.len() < INTENT_QUEUE_CAP {
        assert!(q.push(QueuedSnapEvent::Intent(
            tiler_windows::snapkey::QueuedIntent {
                op: SnapOp::Focus,
                direction: Direction::Left,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick,
            }
        )));
    }
    assert!(q.is_full());
    // Saturated approved chord still swallows, counted as loss.
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_J, false, origin, false, tick, true),
        Some(true)
    );
    assert_eq!(q.dropped, 1);
    // Mask reservation gracefully makes room when full instead of skipping
    // the necessary mask.
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    assert_eq!(q.dropped, 2, "mask room comes from dropped action evidence");
}

#[test]
fn origin_continuation_survives_across_bounded_drains() {
    // The owner persists one verified own-focus advance across bounded
    // drains (MAX_DISPATCH_PER_TICK): a stale second-batch chord for A
    // continues from the verified advance B, while an external focus move
    // rejects and clears the chain.
    let fresh = vec![origin_of(11, "w1"), origin_of(22, "w2")];
    // Batch 1: chord for A with stable foreground dispatches; the owner
    // verifies actuation to B and records the advance.
    let mut advance: Option<SnapOrigin> = None;
    assert_eq!(
        resolve_origin(&origin_of(11, "w1"), Some(11), &fresh, advance.as_ref()),
        OriginVerdict::Dispatch {
            token: "w1".to_owned(),
            continued: false,
        }
    );
    advance = Some(origin_of(22, "w2"));
    // Batch 2 (next drain, same burst): stale chord for A with foreground
    // now exactly at the verified advance B continues from B.
    assert_eq!(
        resolve_origin(&origin_of(11, "w1"), Some(22), &fresh, advance.as_ref()),
        OriginVerdict::Dispatch {
            token: "w2".to_owned(),
            continued: true,
        }
    );
    // External focus to an unrelated window rejects and clears.
    let fresh_external = vec![origin_of(11, "w1"), origin_of(99, "w9")];
    assert_eq!(
        resolve_origin(
            &origin_of(11, "w1"),
            Some(99),
            &fresh_external,
            advance.as_ref()
        ),
        OriginVerdict::Reject("foreground-changed")
    );
    advance = None;
    assert!(advance.is_none(), "external focus clears the chain");
}

#[test]
fn mask_key_is_unassigned_e8_and_fires_once() {
    assert_eq!(VK_MASK, 0xE8);
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    // Plain Win tap never masks.
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut q,
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
    assert!(q.is_empty());
    // Consumed hold fires exactly once at Win-up.
    win_down(&mut m, VK_LWIN);
    let tick = std::time::Instant::now();
    let origin = Some(origin_of(11, "w1"));
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_H,
            false,
            origin.clone(),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, true, origin, false, tick, true),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    assert_eq!(m.mask_attempted, 1);
    stamp_mask_result(&mut m, &mut q, 2, false);
    assert_eq!((m.mask_ok, m.mask_failed), (1, 0));
    stamp_mask_result(&mut m, &mut q, 0, false);
    assert_eq!((m.mask_ok, m.mask_failed), (1, 1));
}

// A chord consumed for A dispatches on A while the foreground still matches.
#[test]
fn origin_verdict_dispatches_on_stable_foreground() {
    let fresh = vec![origin_of(11, "w1"), origin_of(22, "w2")];
    assert_eq!(
        resolve_origin(&origin_of(11, "w1"), Some(11), &fresh, None),
        OriginVerdict::Dispatch {
            token: "w1".to_owned(),
            continued: false,
        }
    );
}

// External focus change: a chord for A never silently acts on unrelated B.
#[test]
fn origin_verdict_rejects_external_focus_change() {
    let fresh = vec![origin_of(11, "w1"), origin_of(22, "w2")];
    assert_eq!(
        resolve_origin(&origin_of(11, "w1"), Some(22), &fresh, None),
        OriginVerdict::Reject("foreground-changed")
    );
    // ... even when the origin window is still managed.
    assert_eq!(
        resolve_origin(&origin_of(11, "w1"), Some(99), &fresh, None),
        OriginVerdict::Reject("foreground-changed")
    );
}

// Owner-caused advance: a prior verified actuation to B in the same batch
// lets the next intent continue from B, preserving rapid repeated navigation.
#[test]
fn origin_verdict_continues_verified_owner_advance() {
    let fresh = vec![origin_of(11, "w1"), origin_of(22, "w2")];
    assert_eq!(
        resolve_origin(
            &origin_of(11, "w1"),
            Some(22),
            &fresh,
            Some(&origin_of(22, "w2")),
        ),
        OriginVerdict::Dispatch {
            token: "w2".to_owned(),
            continued: true,
        }
    );
    // A stale advance (actuation went to C, foreground is B) never authorizes.
    assert_eq!(
        resolve_origin(
            &origin_of(11, "w1"),
            Some(22),
            &fresh,
            Some(&origin_of(33, "w3")),
        ),
        OriginVerdict::Reject("foreground-changed")
    );
}

// Vanished origin: closed or unmanaged since chord time.
#[test]
fn origin_verdict_rejects_vanished_origin() {
    let fresh = vec![origin_of(22, "w2")];
    assert_eq!(
        resolve_origin(&origin_of(11, "w1"), Some(22), &fresh, None),
        OriginVerdict::Reject("origin-vanished")
    );
}

// Identity reuse: the same HWND with a recycled process never matches the
// chord origin, and a recycled advance never authorizes continuation.
#[test]
fn origin_verdict_rejects_recycled_hwnd_identity() {
    let recycled = SnapOrigin {
        hwnd: 11,
        token: "w9".to_owned(),
        pid: 4242,
        creation: "creation-other".to_owned(),
    };
    let fresh = vec![recycled, origin_of(22, "w2")];
    assert_eq!(
        resolve_origin(&origin_of(11, "w1"), Some(11), &fresh, None),
        OriginVerdict::Reject("origin-vanished")
    );
    // A stale advance recorded before the recycle no longer matches the
    // recycled HWND's new token/pid/creation triple.
    assert_eq!(
        resolve_origin(
            &origin_of(22, "w2"),
            Some(11),
            &fresh,
            Some(&origin_of(11, "w1")),
        ),
        OriginVerdict::Reject("foreground-changed")
    );
}

#[test]
fn direction_names_match_engine_wire() {
    assert_eq!(direction_name(Direction::Left), "left");
    assert_eq!(direction_name(Direction::Right), "right");
    assert_eq!(direction_name(Direction::Up), "up");
    assert_eq!(direction_name(Direction::Down), "down");
    assert_eq!(SnapOp::Focus.as_str(), "focus");
    assert_eq!(SnapOp::Move.as_str(), "move");
}

#[test]
fn tile_args_keyboard_defaults_and_flags() {
    let base = parse_tile_args(&strings(&["--user-start"])).expect("parsed");
    assert!(base.user_start && !base.trace);
    assert!(!base.no_keyboard_snap_takeover, "takeover defaults on");
    assert!(!base.allow_win_l, "win+l defaults gated");
    let off = parse_tile_args(&strings(&["--user-start", "--no-keyboard-snap-takeover"]))
        .expect("parsed");
    assert!(off.no_keyboard_snap_takeover);
    let l = parse_tile_args(&strings(&["--user-start", "--allow-win-l"])).expect("parsed");
    assert!(l.allow_win_l);
    assert!(parse_tile_args(&strings(&["--user-start", "--allowlist", "a.json"])).is_err());
    assert!(parse_tile_args(&strings(&["--no-keyboard-snap-takeover"])).is_err());
    // Proof parser takes no keyboard flags: the proof contract is unchanged.
    assert!(
        tiler_windows::tiling::parse_tile_proof_args(&strings(&[
            "--allowlist",
            "a.json",
            "--allow-win-l"
        ]))
        .is_err()
    );
}

#[test]
fn digit_chords_select_and_send_on_same_vk() {
    // US symbols share the digit VK: Shift flips select into send.
    for index in [0u8, 1, 5, 9] {
        let vk = VK_0 + u32::from(index);
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        let select = push_workspace(&mut m, vk, false, true, false).expect("select down");
        assert_eq!((select.op, select.index), (WorkspaceOp::Select, index));
        assert!(select.consumed && select.announce);
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_RWIN);
        push_snap(&mut m, tiler_windows::snapkey::VK_SHIFT, false, true, false);
        let send = push_workspace(&mut m, vk, false, true, false).expect("send down");
        assert_eq!((send.op, send.index), (WorkspaceOp::Send, index));
        assert!(send.consumed && send.announce);
    }
}

#[test]
fn digit_chords_share_modifier_mask_and_origin_authority() {
    // One machine: Ctrl/Alt or missing Win refuse, injected never classifies,
    // the op fixes at down time, and a consumed digit arms the E8 mask.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    assert_eq!(push_workspace(&mut m, VK_0 + 1, false, true, false), None);
    push_snap(&mut m, VK_CONTROL, true, true, false);
    assert_eq!(push_workspace(&mut m, VK_0 + 2, true, true, false), None);
    assert_eq!(push_workspace(&mut m, VK_0 + 3, false, true, true), None);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_LSHIFT, false, true, false);
    let down = push_workspace(&mut m, VK_0 + 3, false, true, false).expect("send down");
    assert_eq!(down.op, WorkspaceOp::Send);
    push_snap(&mut m, VK_LSHIFT, true, true, false);
    let up = push_workspace(&mut m, VK_0 + 3, true, true, false).expect("send up");
    assert_eq!((up.op, up.consumed), (WorkspaceOp::Send, true));
    assert_eq!(up.edge, SnapEdge::Up);
    // Authentic: both select and send consume unmanaged at interception;
    // the owner rechecks fresh identity before any send acts (unmanaged send
    // settles as unmanaged, never manipulates).
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_workspace(&mut m, VK_0 + 4, false, false, false).expect("global select down");
    assert_eq!(down.op, WorkspaceOp::Select);
    assert!(down.consumed && down.announce);
    let repeat =
        push_workspace(&mut m, VK_0 + 4, false, false, false).expect("global select repeat");
    assert!(repeat.consumed && repeat.announce);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let down = push_workspace(&mut m, VK_0 + 4, false, false, false).expect("unmanaged send down");
    assert_eq!(down.op, WorkspaceOp::Send);
    assert!(down.consumed && down.announce);
    let repeat = push_workspace(&mut m, VK_0 + 4, false, true, false).expect("repeat");
    assert!(repeat.consumed);
    // Takeover off passes everything through without consuming.
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    win_down(&mut m, VK_LWIN);
    let down = push_workspace(&mut m, VK_0 + 5, false, true, false).expect("off down");
    assert!(!down.consumed);
}

#[test]
fn global_select_consumes_without_managed_origin() {
    // Authentic: both Win+1 and Win+Shift+1 from unmanaged foreground consume
    // with no origin; the owner settles send as unmanaged without acting.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_0 + 1, false, None, false, tick, true),
        Some(true)
    );
    match q.pop_front().expect("workspace intent") {
        QueuedSnapEvent::Workspace(intent) => {
            assert_eq!((intent.op, intent.index), (WorkspaceOp::Select, 1));
            assert!(intent.consumed && intent.announce);
            assert!(intent.origin.is_none());
        }
        _ => panic!("expected workspace intent"),
    }
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_SHIFT, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_0 + 1, false, None, false, tick, true),
        Some(true)
    );
}

#[test]
fn digit_mask_roundtrip_and_queue_saturation() {
    use tiler_windows::snapkey::{MaskTrigger, QueuedWorkspaceIntent};
    // A consumed digit reserves the mask at Win-up and stamps the result.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_0 + 1,
            false,
            Some(origin_of(7, "w7")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("workspace intent") {
        QueuedSnapEvent::Workspace(QueuedWorkspaceIntent { op, index, .. }) => {
            assert_eq!((op, index), (WorkspaceOp::Select, 1));
        }
        _ => panic!("expected workspace intent"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => match mask.trigger {
            MaskTrigger::Workspace { op, index } => {
                assert_eq!((op, index), (WorkspaceOp::Select, 1));
            }
            _ => panic!("expected workspace mask trigger"),
        },
        _ => panic!("expected mask"),
    }
    // Authentic saturation: the digit still swallows and counts loss.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    push_snap(&mut m, VK_LWIN, false, true, false);
    while q.len() < INTENT_QUEUE_CAP {
        let _ = q.push(QueuedSnapEvent::Mask(tiler_windows::snapkey::QueuedMask {
            trigger: MaskTrigger::Workspace {
                op: WorkspaceOp::Select,
                index: 1,
            },
            tick,
            inserted: 0,
            release_sent: false,
        }));
    }
    assert!(q.is_full());
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_0 + 2,
            false,
            Some(origin_of(8, "w8")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(q.dropped >= 1);
}

#[test]
fn gate_disabled_select_passes_through() {
    // Shortcut-handling gate off: a fresh Win+digit passes through without
    // consuming and its pair passes too, even with takeover on. Inactive
    // tiling (suspended/fullscreen/elevated/gesture, unmanaged foreground)
    // never clears this gate - publication stays takeover-only - so those
    // chords still swallow while the gate holds (see the unmanaged tests).
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_0 + 2, false, None, false, tick, false),
        Some(false)
    );
    match q.pop_front().expect("workspace intent") {
        QueuedSnapEvent::Workspace(intent) => {
            assert_eq!((intent.op, intent.index), (WorkspaceOp::Select, 2));
            assert!(!intent.consumed && !intent.announce);
        }
        _ => panic!("expected workspace intent"),
    }
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_0 + 2, true, None, false, tick, false),
        Some(false),
        "gate-off pair stays passed"
    );
}

#[test]
fn active_unmanaged_select_consumes_without_origin() {
    // Authentic: both select and send consume unmanaged at interception;
    // send without an origin settles as unmanaged without acting.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_0 + 3, false, None, false, tick, true),
        Some(true)
    );
    match q.pop_front().expect("workspace intent") {
        QueuedSnapEvent::Workspace(intent) => {
            assert_eq!((intent.op, intent.index), (WorkspaceOp::Select, 3));
            assert!(intent.consumed && intent.announce);
            assert!(intent.origin.is_none());
        }
        _ => panic!("expected workspace intent"),
    }
    // Same gate, same unmanaged foreground: send still swallows.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_SHIFT, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_0 + 3, false, None, false, tick, true),
        Some(true)
    );
}

#[test]
fn maximize_toggle_consumes_down_repeat_up_with_origin_pairing() {
    // Win+M (KDE Meta+M parity): down consumes with announce, held repeat is
    // swallowed (no re-toggle, like the discrete KDE shortcut), up closes the
    // pair without announce. Win released before the key-up still pairs by
    // origin.
    assert!(is_chord_vk(VK_M));
    assert_eq!(VK_M, 0x4D);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(m.win_held());
    assert!(!m.key_is_down(VK_M));
    let down = push_maximize(&mut m, false, true, false).expect("maximize down");
    assert_eq!(down.edge, SnapEdge::Down);
    assert!(down.consumed && down.announce);
    assert!(m.key_is_down(VK_M));
    let repeat = push_maximize(&mut m, false, true, false).expect("maximize repeat");
    assert_eq!(repeat.edge, SnapEdge::Repeat);
    assert!(repeat.consumed && !repeat.announce);
    // Win released before the key-up still pairs by origin.
    push_snap(&mut m, VK_LWIN, true, true, false);
    let up = push_maximize(&mut m, true, true, false).expect("paired up");
    assert_eq!(up.edge, SnapEdge::Up);
    assert!(up.consumed && !up.announce);
    assert!(!m.key_is_down(VK_M));
    assert_eq!(push_maximize(&mut m, true, true, false), None);
    assert_eq!(
        (m.max_counts.down, m.max_counts.repeat, m.max_counts.up),
        (1, 1, 1)
    );
    assert_eq!(m.max_counts.consumed, 3);
    // Directional catalog untouched by the M chord.
    assert!(m.counts.iter().all(|c| c.down == 0 && c.up == 0));
}

#[test]
fn maximize_modifier_exactness_passes_untracked() {
    // Shift selects the directional move arm: Win+Shift+M passes through
    // untracked, and its paired key-up passes too. Ctrl/Alt, missing Win,
    // bare M, and injected M never classify and never arm the mask.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_maximize(&mut m, false, true, false), None);
    assert_eq!(push_maximize(&mut m, true, true, false), None);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    assert_eq!((m.max_counts.down, m.max_counts.up), (0, 0));
    for mod_vk in [VK_CONTROL, VK_MENU, VK_LMENU] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        assert_eq!(push_maximize(&mut m, false, true, false), None);
        assert_eq!(push_maximize(&mut m, true, true, false), None);
        push_snap(&mut m, mod_vk, true, true, false);
    }
    let mut m = SnapClassify::new(takeover());
    assert_eq!(push_maximize(&mut m, false, true, false), None);
    assert_eq!(push_maximize(&mut m, false, true, true), None);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_maximize(&mut m, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut SnapQueue::new(),
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
}

#[test]
fn maximize_unmanaged_consumes_gate_off_passes() {
    // Unmanaged foreground with the gate on still swallows at interception
    // (the owner settles without acting); takeover off stays passthrough,
    // and a fresh chord with the shortcut gate off passes through.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_maximize(&mut m, false, false, false).expect("unmanaged down");
    assert!(down.consumed && down.announce);
    let repeat = push_maximize(&mut m, false, true, false).expect("repeat");
    assert!(repeat.consumed);
    let up = push_maximize(&mut m, true, true, false).expect("paired up");
    assert!(up.consumed);
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    win_down(&mut m, VK_LWIN);
    let down = push_maximize(&mut m, false, true, false).expect("logged");
    assert!(!down.consumed && !down.announce);
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_M, false, None, false, tick, false),
        Some(false)
    );
    match q.pop_front().expect("maximize intent") {
        QueuedSnapEvent::Maximize(intent) => assert!(!intent.consumed),
        _ => panic!("expected maximize intent"),
    }
}

#[test]
fn maximize_arms_start_menu_mask_and_survives_saturation() {
    use tiler_windows::snapkey::{MaskTrigger, QueuedMaximizeIntent, SnapQueue};
    // A consumed Win+M reserves the E8 mask at Win-up with trigger evidence.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_M,
            false,
            Some(origin_of(7, "w7")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("maximize intent") {
        QueuedSnapEvent::Maximize(QueuedMaximizeIntent { origin, .. }) => {
            assert_eq!(origin, Some(origin_of(7, "w7")));
        }
        _ => panic!("expected maximize intent"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, MaskTrigger::Maximize),
        _ => panic!("expected mask"),
    }
    // Authentic saturation: the toggle still swallows and counts loss,
    // while an earlier consumed toggle's mask stays armed (mask reservation
    // makes room when full).
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    push_snap(&mut m, VK_LWIN, false, true, false);
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_M,
            false,
            Some(origin_of(9, "w9")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    while q.len() < INTENT_QUEUE_CAP {
        let _ = q.push(QueuedSnapEvent::Mask(tiler_windows::snapkey::QueuedMask {
            trigger: MaskTrigger::Maximize,
            tick,
            inserted: 0,
            release_sent: false,
        }));
    }
    assert!(q.is_full());
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_M,
            false,
            Some(origin_of(9, "w9")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(q.dropped >= 1);
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
}

#[test]
fn orientation_toggle_consumes_down_repeat_up_with_origin_pairing() {
    // Win+O (KDE Meta+O parity, item 4): down consumes with announce, held
    // repeat is swallowed (no re-toggle, like the discrete KDE shortcut),
    // up closes the pair without announce. Win released before the key-up
    // still pairs by origin. Repeat re-dispatch stays native-journey TBD
    // (tentative): the hold swallows without dispatching.
    use tiler_windows::snapkey::{VK_O, is_orientation_vk};
    assert!(is_chord_vk(VK_O));
    assert!(is_orientation_vk(VK_O));
    assert_eq!(VK_O, 0x4F);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(m.win_held());
    assert!(!m.key_is_down(VK_O));
    let down = push_orientation(&mut m, false, true, false).expect("orientation down");
    assert_eq!(down.edge, SnapEdge::Down);
    assert!(down.consumed && down.announce);
    assert!(m.key_is_down(VK_O));
    let repeat = push_orientation(&mut m, false, true, false).expect("orientation repeat");
    assert_eq!(repeat.edge, SnapEdge::Repeat);
    assert!(repeat.consumed && !repeat.announce);
    // Win released before the key-up still pairs by origin.
    push_snap(&mut m, VK_LWIN, true, true, false);
    let up = push_orientation(&mut m, true, true, false).expect("paired up");
    assert_eq!(up.edge, SnapEdge::Up);
    assert!(up.consumed && !up.announce);
    assert!(!m.key_is_down(VK_O));
    assert_eq!(push_orientation(&mut m, true, true, false), None);
    assert_eq!(
        (
            m.orientation_counts.down,
            m.orientation_counts.repeat,
            m.orientation_counts.up
        ),
        (1, 1, 1)
    );
    assert_eq!(m.orientation_counts.consumed, 3);
    // Directional catalog untouched by the O chord.
    assert!(m.counts.iter().all(|c| c.down == 0 && c.up == 0));
}

#[test]
fn orientation_modifier_exactness_passes_untracked() {
    // Only exact unshifted Win+O classifies: Shift/Ctrl/Alt, missing Win,
    // bare O, and injected O never classify and never arm the mask.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_orientation(&mut m, false, true, false), None);
    assert_eq!(push_orientation(&mut m, true, true, false), None);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    assert_eq!((m.orientation_counts.down, m.orientation_counts.up), (0, 0));
    for mod_vk in [VK_CONTROL, VK_MENU, VK_LMENU] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        assert_eq!(push_orientation(&mut m, false, true, false), None);
        assert_eq!(push_orientation(&mut m, true, true, false), None);
        push_snap(&mut m, mod_vk, true, true, false);
    }
    let mut m = SnapClassify::new(takeover());
    assert_eq!(push_orientation(&mut m, false, true, false), None);
    assert_eq!(push_orientation(&mut m, false, true, true), None);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_orientation(&mut m, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut SnapQueue::new(),
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
}

#[test]
fn workspace_tiling_toggle_consumes_down_repeat_up_with_origin_pairing() {
    // Win+Y (KDE Meta+Y parity, item 21): down consumes with announce, held
    // repeat is swallowed (no mode pingpong), up closes the pair without
    // announce. Win released before the key-up still pairs by origin.
    use tiler_windows::snapkey::{VK_Y, is_workspace_tiling_vk};
    assert!(is_chord_vk(VK_Y));
    assert!(is_workspace_tiling_vk(VK_Y));
    assert_eq!(VK_Y, 0x59);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(m.win_held());
    assert!(!m.key_is_down(VK_Y));
    let down = push_workspace_tiling(&mut m, false, true, false).expect("tiling down");
    assert_eq!(down.edge, SnapEdge::Down);
    assert!(down.consumed && down.announce);
    assert!(m.key_is_down(VK_Y));
    let repeat = push_workspace_tiling(&mut m, false, true, false).expect("tiling repeat");
    assert_eq!(repeat.edge, SnapEdge::Repeat);
    assert!(repeat.consumed && !repeat.announce);
    // Win released before the key-up still pairs by origin.
    push_snap(&mut m, VK_LWIN, true, true, false);
    let up = push_workspace_tiling(&mut m, true, true, false).expect("paired up");
    assert_eq!(up.edge, SnapEdge::Up);
    assert!(up.consumed && !up.announce);
    assert!(!m.key_is_down(VK_Y));
    assert_eq!(push_workspace_tiling(&mut m, true, true, false), None);
    assert_eq!(
        (
            m.workspace_tiling_counts.down,
            m.workspace_tiling_counts.repeat,
            m.workspace_tiling_counts.up
        ),
        (1, 1, 1)
    );
    assert_eq!(m.workspace_tiling_counts.consumed, 3);
    // Directional catalog untouched by the Y chord.
    assert!(m.counts.iter().all(|c| c.down == 0 && c.up == 0));
}

#[test]
fn workspace_tiling_modifier_exactness_passes_untracked() {
    // Only exact unshifted Win+Y classifies: Shift/Ctrl/Alt, missing Win,
    // bare Y, and injected Y never classify and never arm the mask.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_workspace_tiling(&mut m, false, true, false), None);
    assert_eq!(push_workspace_tiling(&mut m, true, true, false), None);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    assert_eq!(
        (m.workspace_tiling_counts.down, m.workspace_tiling_counts.up),
        (0, 0)
    );
    for mod_vk in [VK_CONTROL, VK_MENU, VK_LMENU] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        assert_eq!(push_workspace_tiling(&mut m, false, true, false), None);
        assert_eq!(push_workspace_tiling(&mut m, true, true, false), None);
        push_snap(&mut m, mod_vk, true, true, false);
    }
    let mut m = SnapClassify::new(takeover());
    assert_eq!(push_workspace_tiling(&mut m, false, true, false), None);
    assert_eq!(push_workspace_tiling(&mut m, false, true, true), None);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_workspace_tiling(&mut m, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut SnapQueue::new(),
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
}

#[test]
fn workspace_tiling_unmanaged_consumes_gate_off_passes() {
    // Unmanaged foreground with the gate on still swallows at interception
    // (the owner settles without acting); takeover off stays passthrough,
    // and a fresh chord with the shortcut gate off passes through.
    use tiler_windows::snapkey::VK_Y;
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_workspace_tiling(&mut m, false, false, false).expect("unmanaged down");
    assert!(down.consumed && down.announce);
    let repeat = push_workspace_tiling(&mut m, false, true, false).expect("repeat");
    assert!(repeat.consumed);
    let up = push_workspace_tiling(&mut m, true, true, false).expect("paired up");
    assert!(up.consumed);
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    win_down(&mut m, VK_LWIN);
    let down = push_workspace_tiling(&mut m, false, true, false).expect("logged");
    assert!(!down.consumed && !down.announce);
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_Y, false, None, false, tick, false),
        Some(false)
    );
    match q.pop_front().expect("workspace-tiling intent") {
        QueuedSnapEvent::WorkspaceTiling(intent) => assert!(!intent.consumed),
        _ => panic!("expected workspace-tiling intent"),
    }
}

#[test]
fn workspace_tiling_arms_start_menu_mask() {
    use tiler_windows::snapkey::{MaskTrigger, QueuedWorkspaceTilingIntent, SnapQueue, VK_Y};
    // A consumed Win+Y reserves the E8 mask at Win-up with trigger evidence.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_Y,
            false,
            Some(origin_of(7, "w7")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("workspace-tiling intent") {
        QueuedSnapEvent::WorkspaceTiling(QueuedWorkspaceTilingIntent { origin, .. }) => {
            assert_eq!(origin, Some(origin_of(7, "w7")));
        }
        _ => panic!("expected workspace-tiling intent"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, MaskTrigger::WorkspaceTiling),
        _ => panic!("expected mask"),
    }
}

#[test]
fn workspace_tiling_held_rebind_disable_gate_repeat_but_keep_pair() {
    // Rebound workspace-tiling (Win+U -> WorkspaceTiling/Y) dispatches;
    // disabling the canonical mid-hold gates repeat dispatch but keeps the
    // consumed pair. Mirrors the orientation rebind refusal shape.
    use tiler_windows::snapkey::{ChordAction, ChordDisable, ChordRemap, VK_Y};
    const VK_U: u32 = 0x55;
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: ChordAction::WorkspaceTiling,
        to_vk: VK_Y,
    }]);
    win_down(&mut m, VK_LWIN);
    let down = match SnapClassify::push(&mut m, VK_U, false, true, false).expect("down") {
        Classified::WorkspaceTiling(intent) => intent,
        other => panic!("expected workspace-tiling, got {other:?}"),
    };
    assert!(down.consumed && down.announce);
    m.set_disabled(vec![ChordDisable {
        vk: VK_Y,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    let repeat = match SnapClassify::push(&mut m, VK_U, false, true, false).expect("repeat") {
        Classified::WorkspaceTiling(intent) => intent,
        other => panic!("expected workspace-tiling, got {other:?}"),
    };
    assert!(repeat.consumed && !repeat.announce);
    match SnapClassify::push(&mut m, VK_U, true, true, false).expect("up") {
        Classified::WorkspaceTiling(up) => assert!(up.consumed && !up.announce),
        other => panic!("expected workspace-tiling, got {other:?}"),
    }
}

#[test]
fn orientation_unmanaged_consumes_gate_off_passes() {
    // Unmanaged foreground with the gate on still swallows at interception
    // (the owner settles without acting); takeover off stays passthrough,
    // and a fresh chord with the shortcut gate off passes through.
    use tiler_windows::snapkey::VK_O;
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_orientation(&mut m, false, false, false).expect("unmanaged down");
    assert!(down.consumed && down.announce);
    let repeat = push_orientation(&mut m, false, true, false).expect("repeat");
    assert!(repeat.consumed);
    let up = push_orientation(&mut m, true, true, false).expect("paired up");
    assert!(up.consumed);
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    win_down(&mut m, VK_LWIN);
    let down = push_orientation(&mut m, false, true, false).expect("logged");
    assert!(!down.consumed && !down.announce);
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_O, false, None, false, tick, false),
        Some(false)
    );
    match q.pop_front().expect("orientation intent") {
        QueuedSnapEvent::Orientation(intent) => assert!(!intent.consumed),
        _ => panic!("expected orientation intent"),
    }
}

#[test]
fn orientation_arms_start_menu_mask_and_survives_saturation() {
    use tiler_windows::snapkey::{
        MaskTrigger, QueuedMask, QueuedOrientationIntent, SnapQueue, VK_O,
    };
    // A consumed Win+O reserves the E8 mask at Win-up with trigger evidence.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_O,
            false,
            Some(origin_of(7, "w7")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("orientation intent") {
        QueuedSnapEvent::Orientation(QueuedOrientationIntent { origin, .. }) => {
            assert_eq!(origin, Some(origin_of(7, "w7")));
        }
        _ => panic!("expected orientation intent"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, MaskTrigger::Orientation),
        _ => panic!("expected mask"),
    }
    // Authentic saturation: the toggle still swallows and counts loss,
    // while an earlier consumed toggle's mask stays armed (mask reservation
    // makes room when full).
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    push_snap(&mut m, VK_LWIN, false, true, false);
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_O,
            false,
            Some(origin_of(9, "w9")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    while q.len() < INTENT_QUEUE_CAP {
        let _ = q.push(QueuedSnapEvent::Mask(QueuedMask {
            trigger: MaskTrigger::Orientation,
            tick,
            inserted: 0,
            release_sent: false,
        }));
    }
    assert!(q.is_full());
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_O,
            false,
            Some(origin_of(9, "w9")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(q.dropped >= 1);
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
}

#[test]
fn orientation_held_rebind_disable_gate_repeat_but_keep_pair() {
    // Rebound orientation (Win+U -> Orientation/O) dispatches; removing the
    // mapping mid-hold swallows repeats without dispatching while the pair
    // still closes consumed; disabling the canonical mid-hold does the same;
    // fresh presses afterwards pass through.
    use tiler_windows::snapkey::{ChordAction, ChordDisable, ChordRemap, VK_O};
    const VK_U: u32 = 0x55;
    let remap = || {
        vec![ChordRemap {
            from_vk: VK_U,
            from_shift: false,
            from_ctrl: false,
            from_alt: false,
            action: ChordAction::Orientation,
            to_vk: VK_O,
        }]
    };
    let mut m = SnapClassify::new(takeover());
    m.set_remap(remap());
    win_down(&mut m, VK_LWIN);
    match SnapClassify::push(&mut m, VK_U, false, true, false).expect("rebound down") {
        Classified::Orientation(intent) => assert!(intent.consumed && intent.announce),
        other => panic!("expected orientation, got {other:?}"),
    }
    m.set_remap(Vec::new());
    let repeat = SnapClassify::push(&mut m, VK_U, false, true, false).expect("repeat");
    assert!(repeat.consumed() && !repeat.announce());
    let up = SnapClassify::push(&mut m, VK_U, true, true, false).expect("up");
    assert!(up.consumed());
    assert_eq!(SnapClassify::push(&mut m, VK_U, false, true, false), None);
    // Canonical disable mid-hold gates dispatch but keeps the pair.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_orientation(&mut m, false, true, false).expect("down");
    assert!(down.consumed && down.announce);
    m.set_disabled(vec![ChordDisable {
        vk: VK_O,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    let repeat = push_orientation(&mut m, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    let up = push_orientation(&mut m, true, true, false).expect("up");
    assert!(up.consumed);
}

#[test]
fn fullscreen_toggle_consumes_down_repeat_up_with_origin_pairing() {
    // Win+F11 (KDE Meta+F11 parity): down consumes with announce, held repeat
    // is swallowed (no re-toggle, like the discrete shortcut), up closes the
    // pair without announce. Win released before the key-up still pairs by
    // origin.
    assert!(is_chord_vk(VK_F11));
    assert!(is_fullscreen_vk(VK_F11));
    assert_eq!(VK_F11, 0x7A);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(m.win_held());
    assert!(!m.key_is_down(VK_F11));
    let down = push_fullscreen(&mut m, false, true, false).expect("fullscreen down");
    assert_eq!(down.edge, SnapEdge::Down);
    assert!(down.consumed && down.announce);
    assert!(m.key_is_down(VK_F11));
    let repeat = push_fullscreen(&mut m, false, true, false).expect("fullscreen repeat");
    assert_eq!(repeat.edge, SnapEdge::Repeat);
    assert!(repeat.consumed && !repeat.announce);
    // Win released before the key-up still pairs by origin.
    push_snap(&mut m, VK_LWIN, true, true, false);
    let up = push_fullscreen(&mut m, true, true, false).expect("paired up");
    assert_eq!(up.edge, SnapEdge::Up);
    assert!(up.consumed && !up.announce);
    assert!(!m.key_is_down(VK_F11));
    assert_eq!(push_fullscreen(&mut m, true, true, false), None);
    assert_eq!(
        (
            m.fullscreen_counts.down,
            m.fullscreen_counts.repeat,
            m.fullscreen_counts.up
        ),
        (1, 1, 1)
    );
    assert_eq!(m.fullscreen_counts.consumed, 3);
    // Directional and maximize catalogs untouched by the F11 chord.
    assert!(m.counts.iter().all(|c| c.down == 0 && c.up == 0));
    assert_eq!((m.max_counts.down, m.max_counts.up), (0, 0));
}

#[test]
fn fullscreen_modifier_exactness_passes_untracked() {
    // Any Shift/Ctrl/Alt, missing Win, bare F11, and injected F11 never
    // classify and never arm the mask: there is no shifted fullscreen arm.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_fullscreen(&mut m, false, true, false), None);
    assert_eq!(push_fullscreen(&mut m, true, true, false), None);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    assert_eq!((m.fullscreen_counts.down, m.fullscreen_counts.up), (0, 0));
    for mod_vk in [VK_CONTROL, VK_MENU, VK_LMENU] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        assert_eq!(push_fullscreen(&mut m, false, true, false), None);
        assert_eq!(push_fullscreen(&mut m, true, true, false), None);
        push_snap(&mut m, mod_vk, true, true, false);
    }
    let mut m = SnapClassify::new(takeover());
    assert_eq!(push_fullscreen(&mut m, false, true, false), None);
    assert_eq!(push_fullscreen(&mut m, false, true, true), None);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_fullscreen(&mut m, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut SnapQueue::new(),
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
}

#[test]
fn fullscreen_unmanaged_consumes_gate_off_passes() {
    // Unmanaged foreground with the gate on still swallows at interception
    // (the owner settles without acting); takeover off stays passthrough,
    // and a fresh chord with the shortcut gate off passes through.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_fullscreen(&mut m, false, false, false).expect("unmanaged down");
    assert!(down.consumed && down.announce);
    let repeat = push_fullscreen(&mut m, false, true, false).expect("repeat");
    assert!(repeat.consumed);
    let up = push_fullscreen(&mut m, true, true, false).expect("paired up");
    assert!(up.consumed);
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    win_down(&mut m, VK_LWIN);
    let down = push_fullscreen(&mut m, false, true, false).expect("logged");
    assert!(!down.consumed && !down.announce);
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_F11, false, None, false, tick, false),
        Some(false)
    );
    match q.pop_front().expect("fullscreen intent") {
        QueuedSnapEvent::Fullscreen(intent) => assert!(!intent.consumed),
        _ => panic!("expected fullscreen intent"),
    }
}

#[test]
fn fullscreen_arms_start_menu_mask() {
    use tiler_windows::snapkey::{MaskTrigger, QueuedFullscreenIntent, SnapQueue};
    // A consumed Win+F11 reserves the E8 mask at Win-up with trigger evidence.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_F11,
            false,
            Some(origin_of(7, "w7")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("fullscreen intent") {
        QueuedSnapEvent::Fullscreen(QueuedFullscreenIntent { origin, .. }) => {
            assert_eq!(origin, Some(origin_of(7, "w7")));
        }
        _ => panic!("expected fullscreen intent"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, MaskTrigger::Fullscreen),
        _ => panic!("expected mask"),
    }
}

#[test]
fn float_toggle_consumes_down_repeat_up_with_origin_pairing() {
    use tiler_windows::snapkey::VK_G;
    // Win+G (KDE Meta+G parity): down consumes with announce, held repeat is
    // swallowed (no re-toggle, like the discrete KDE shortcut), up closes the
    // pair without announce. Win released before the key-up still pairs by
    // origin.
    assert!(is_chord_vk(VK_G));
    assert_eq!(VK_G, 0x47);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(m.win_held());
    assert!(!m.key_is_down(VK_G));
    let down = push_float(&mut m, false, true, false).expect("float down");
    assert_eq!(down.edge, SnapEdge::Down);
    assert!(down.consumed && down.announce);
    assert!(m.key_is_down(VK_G));
    let repeat = push_float(&mut m, false, true, false).expect("float repeat");
    assert_eq!(repeat.edge, SnapEdge::Repeat);
    assert!(repeat.consumed && !repeat.announce);
    // Win released before the key-up still pairs by origin.
    push_snap(&mut m, VK_LWIN, true, true, false);
    let up = push_float(&mut m, true, true, false).expect("paired up");
    assert_eq!(up.edge, SnapEdge::Up);
    assert!(up.consumed && !up.announce);
    assert!(!m.key_is_down(VK_G));
    assert_eq!(push_float(&mut m, true, true, false), None);
    assert_eq!(
        (
            m.float_counts.down,
            m.float_counts.repeat,
            m.float_counts.up
        ),
        (1, 1, 1)
    );
    assert_eq!(m.float_counts.consumed, 3);
    // Directional catalog untouched by the G chord.
    assert!(m.counts.iter().all(|c| c.down == 0 && c.up == 0));
    assert_eq!((m.max_counts.down, m.max_counts.up), (0, 0));
}

#[test]
fn float_modifier_exactness_passes_untracked() {
    // Shifted G routes to the sticky arm (never the float arm): the float
    // helper sees no float chord here. Ctrl/Alt, missing Win, bare G, and
    // injected G never classify and never arm the mask.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_float(&mut m, false, true, false), None);
    // The paired key-up closes the sticky hold, not the float hold.
    assert_eq!(push_float(&mut m, true, true, false), None);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    assert_eq!((m.float_counts.down, m.float_counts.up), (0, 0));
    // A fresh shifted hold arms sticky exactly once.
    assert_eq!(m.sticky_counts.down, 1);
    assert_eq!(m.sticky_counts.up, 1);
    for mod_vk in [VK_CONTROL, VK_MENU, VK_LMENU] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        assert_eq!(push_float(&mut m, false, true, false), None);
        assert_eq!(push_float(&mut m, true, true, false), None);
        push_snap(&mut m, mod_vk, true, true, false);
    }
    let mut m = SnapClassify::new(takeover());
    assert_eq!(push_float(&mut m, false, true, false), None);
    assert_eq!(push_float(&mut m, false, true, true), None);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_float(&mut m, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut SnapQueue::new(),
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
}

#[test]
fn float_unmanaged_consumes_gate_off_passes() {
    use tiler_windows::snapkey::VK_G;
    // Unmanaged foreground with the gate on still swallows at interception
    // (the owner settles without acting); takeover off stays passthrough,
    // and a fresh chord with the shortcut gate off passes through.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_float(&mut m, false, false, false).expect("unmanaged down");
    assert!(down.consumed && down.announce);
    let repeat = push_float(&mut m, false, true, false).expect("repeat");
    assert!(repeat.consumed);
    let up = push_float(&mut m, true, true, false).expect("paired up");
    assert!(up.consumed);
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    win_down(&mut m, VK_LWIN);
    let down = push_float(&mut m, false, true, false).expect("logged");
    assert!(!down.consumed && !down.announce);
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_G, false, None, false, tick, false),
        Some(false)
    );
    match q.pop_front().expect("float intent") {
        QueuedSnapEvent::Float(intent) => assert!(!intent.consumed),
        _ => panic!("expected float intent"),
    }
}

#[test]
fn float_arms_start_menu_mask() {
    use tiler_windows::snapkey::{MaskTrigger, QueuedFloatIntent, SnapQueue, VK_G};
    // A consumed Win+G reserves the E8 mask at Win-up with trigger evidence.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_G,
            false,
            Some(origin_of(7, "w7")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("float intent") {
        QueuedSnapEvent::Float(QueuedFloatIntent { origin, .. }) => {
            assert_eq!(origin, Some(origin_of(7, "w7")));
        }
        _ => panic!("expected float intent"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, MaskTrigger::Float),
        _ => panic!("expected mask"),
    }
}

#[test]
fn sticky_toggle_consumes_down_repeat_up_with_origin_pairing() {
    use tiler_windows::snapkey::{SnapEdge, VK_G};
    // Win+Shift+G (KDE Meta+Shift+G parity): down consumes with announce,
    // held repeat is swallowed (no re-toggle), up closes the pair without
    // announce. Shift released before the key-up still pairs by origin.
    assert!(is_chord_vk(VK_G));
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert!(!m.key_is_down(VK_G));
    let down = push_sticky(&mut m, false, true, false).expect("sticky down");
    assert_eq!(down.edge, SnapEdge::Down);
    assert!(down.consumed && down.announce);
    assert!(m.key_is_down(VK_G));
    // The shared G key has one hold: the armed sticky hold owns repeats.
    let repeat = push_sticky(&mut m, false, true, false).expect("sticky repeat");
    assert_eq!(repeat.edge, SnapEdge::Repeat);
    assert!(repeat.consumed && !repeat.announce);
    // Shift released before the key-up still pairs by origin (op fixed).
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let up = push_sticky(&mut m, true, true, false).expect("paired up");
    assert_eq!(up.edge, SnapEdge::Up);
    assert!(up.consumed && !up.announce);
    assert!(!m.key_is_down(VK_G));
    assert_eq!(push_sticky(&mut m, true, true, false), None);
    assert_eq!(
        (
            m.sticky_counts.down,
            m.sticky_counts.repeat,
            m.sticky_counts.up
        ),
        (1, 1, 1)
    );
    assert_eq!(m.sticky_counts.consumed, 3);
    // Float counts untouched by the shifted chord.
    assert_eq!((m.float_counts.down, m.float_counts.up), (0, 0));
}

#[test]
fn sticky_modifier_exactness_and_mask() {
    use tiler_windows::snapkey::{MaskTrigger, QueuedStickyIntent, SnapQueue, VK_G};
    // Unshifted G never arms sticky; Ctrl/Alt, missing Win, bare G, and
    // injected G never classify sticky and never arm the mask.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_sticky(&mut m, false, true, false), None);
    assert_eq!(push_sticky(&mut m, true, true, false), None);
    assert_eq!((m.sticky_counts.down, m.sticky_counts.up), (0, 0));
    for mod_vk in [VK_CONTROL, VK_MENU, VK_LMENU] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, mod_vk, false, true, false);
        push_snap(&mut m, VK_SHIFT, false, true, false);
        assert_eq!(push_sticky(&mut m, false, true, false), None);
        assert_eq!(push_sticky(&mut m, true, true, false), None);
        push_snap(&mut m, mod_vk, true, true, false);
        push_snap(&mut m, VK_SHIFT, true, true, false);
    }
    let mut m = SnapClassify::new(takeover());
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_sticky(&mut m, false, true, false), None);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_sticky(&mut m, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut SnapQueue::new(),
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
    // A consumed Win+Shift+G reserves the E8 mask with sticky evidence.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_SHIFT, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_G,
            false,
            Some(origin_of(9, "w9")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("sticky intent") {
        QueuedSnapEvent::Sticky(QueuedStickyIntent { origin, .. }) => {
            assert_eq!(origin, Some(origin_of(9, "w9")));
        }
        _ => panic!("expected sticky intent"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, MaskTrigger::Sticky),
        _ => panic!("expected mask"),
    }
}

#[test]
fn sticky_unmanaged_consumes_gate_off_passes() {
    // Unmanaged foreground with the gate on still swallows at interception
    // (the owner settles without acting); a fresh chord with the shortcut
    // gate off passes through and never arms a hold.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let down = push_sticky(&mut m, false, false, false).expect("unmanaged down");
    assert!(down.consumed && down.announce);
    let repeat = push_sticky(&mut m, false, true, false).expect("repeat");
    assert!(repeat.consumed);
    let up = push_sticky(&mut m, true, true, false).expect("paired up");
    assert!(up.consumed);
    let mut m = SnapClassify::new(takeover());
    m.set_gate_active(false);
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let down = push_sticky(&mut m, false, true, false).expect("gate-off down");
    assert!(!down.consumed && !down.announce);
    let up = push_sticky(&mut m, true, true, false).expect("gate-off up");
    assert!(!up.consumed);
}

#[test]
fn authentic_matrix_every_arm_consumes_unmanaged() {
    use tiler_windows::snapkey::{VK_G, VK_M};
    // Directional focus arms (except default Win+L): unmanaged still swallows.
    for vk in [VK_H, VK_J, VK_K, VK_LEFT, VK_DOWN, VK_UP, VK_RIGHT] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        let down = push_snap(&mut m, vk, false, false, false).expect("unmanaged focus down");
        assert!(
            down.consumed && down.announce,
            "vk {vk} focus must swallow unmanaged"
        );
        let up = push_snap(&mut m, vk, true, false, false).expect("unmanaged focus up");
        assert!(up.consumed, "vk {vk} focus up must stay swallowed");
    }
    // Directional move arms including Shift+L: unmanaged still swallows.
    for vk in [VK_H, VK_J, VK_K, VK_L, VK_LEFT, VK_DOWN, VK_UP, VK_RIGHT] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, VK_SHIFT, false, false, false);
        let down = push_snap(&mut m, vk, false, false, false).expect("unmanaged move down");
        assert_eq!(down.op, SnapOp::Move, "vk {vk} must resolve move");
        assert!(down.consumed, "vk {vk} move must swallow unmanaged");
    }
    // Digits select/send unmanaged.
    for index in [0u8, 1, 5, 9] {
        let vk = VK_0 + u32::from(index);
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        let select = push_workspace(&mut m, vk, false, false, false).expect("select down");
        assert_eq!(select.op, WorkspaceOp::Select);
        assert!(select.consumed);
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, VK_SHIFT, false, false, false);
        let send = push_workspace(&mut m, vk, false, false, false).expect("send down");
        assert_eq!(send.op, WorkspaceOp::Send);
        assert!(send.consumed, "send {index} must swallow unmanaged");
    }
    // Toggles unmanaged: M/F11/G/Sticky all swallow.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(
        push_maximize(&mut m, false, false, false)
            .expect("m down")
            .consumed
    );
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(
        push_fullscreen(&mut m, false, false, false)
            .expect("f11 down")
            .consumed
    );
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(
        push_float(&mut m, false, false, false)
            .expect("g down")
            .consumed
    );
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, false, false);
    assert!(
        push_sticky(&mut m, false, false, false)
            .expect("shift+g down")
            .consumed
    );
    assert!(is_chord_vk(VK_M) && is_chord_vk(VK_G) && is_chord_vk(VK_F11));
}

#[test]
fn authentic_hold_persists_across_foreground_gate_and_win_order() {
    // Consumed hold stays swallowed across foreground loss, gate closure,
    // extra modifiers, and both up orderings; op fixed at down.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    // Managed down, then foreground lost + gate closed mid-hold.
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_H,
            false,
            Some(origin_of(1, "w1")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, false, None, false, tick, false),
        Some(true),
        "repeat must stay swallowed across foreground/gate loss"
    );
    // Extra Shift transition on consumed focus hold must not leak or flip op.
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_SHIFT, false, None, false, tick, false),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, false, None, false, tick, false),
        Some(true)
    );
    match q.pop_front().expect("down") {
        QueuedSnapEvent::Intent(i) => assert_eq!((i.op, i.consumed), (SnapOp::Focus, true)),
        _ => panic!("intent"),
    }
    // Win-first ordering: Win-up arms mask, chord-up still pairs swallowed.
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, true, None, false, tick, false),
        Some(true)
    );
    // Key-first ordering on a fresh hold.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_J,
            false,
            Some(origin_of(2, "w2")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_J, true, None, false, tick, false),
        Some(true),
        "key-first up must stay swallowed"
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
}

#[test]
fn authentic_takeover_off_hold_never_starts_dispatch() {
    // Hold began with takeover off: reenabling mid-hold must not start dispatch.
    let mut m = SnapClassify::new(KeyboardConfig::disabled());
    win_down(&mut m, VK_LWIN);
    let down = push_snap(&mut m, VK_H, false, true, false).expect("off down");
    assert!(!down.consumed);
    m.set_enabled(true);
    let rep = push_snap(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(
        !rep.consumed,
        "takeover reenabled mid-hold must not start dispatch"
    );
    let up = push_snap(&mut m, VK_H, true, true, false).expect("up");
    assert!(!up.consumed);
}

#[test]
fn authentic_mask_plain_unowned_consumed_and_saturation() {
    use tiler_windows::snapkey::{MaskTrigger, VK_G};
    let tick = std::time::Instant::now();
    // Plain Win tap never masks.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    assert!(!win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    // Unowned chords never arm the mask (Alt on a non-catalog key still
    // passes; Ctrl now owns the history arm, covered below).
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_MENU, false, true, false);
    assert_eq!(SnapClassify::push(&mut m, VK_M, false, true, false), None);
    push_snap(&mut m, VK_MENU, true, true, false);
    assert!(!win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    // Consumed hold arms exactly one mask per Win hold.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_G,
            false,
            Some(origin_of(3, "w3")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    assert_eq!(m.mask_attempted, 1);
    match q.pop_front().expect("float") {
        QueuedSnapEvent::Float(_) => {}
        _ => panic!("float"),
    }
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, MaskTrigger::Float),
        _ => panic!("mask"),
    }
    stamp_mask_result(&mut m, &mut q, 2, false);
    assert_eq!((m.mask_ok, m.mask_failed), (1, 0));
    // Saturation: still swallows, drops evidence, mask makes room.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    push_snap(&mut m, VK_LWIN, false, true, false);
    while q.len() < INTENT_QUEUE_CAP {
        assert!(q.push(QueuedSnapEvent::Intent(
            tiler_windows::snapkey::QueuedIntent {
                op: SnapOp::Focus,
                direction: Direction::Left,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick,
            }
        )));
    }
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_H,
            false,
            Some(origin_of(4, "w4")),
            false,
            tick,
            true
        ),
        Some(true),
        "saturated owned chord must still swallow"
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
}

#[test]
fn gate_disabled_hold_never_starts_dispatch() {
    // Shortcut gate off at down: the hold starts passed and reenabling the
    // gate mid-hold must not steal it - repeats and the paired up stay
    // passed. A fresh chord after reenabling consumes normally.
    let mut m = SnapClassify::new(takeover());
    m.set_gate_active(false);
    win_down(&mut m, VK_LWIN);
    let down = push_snap(&mut m, VK_H, false, true, false).expect("gate-off down");
    assert!(!down.consumed && !down.announce);
    m.set_gate_active(true);
    let rep = push_snap(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(
        !rep.consumed && !rep.announce,
        "gate reenabled mid-hold must not start dispatch"
    );
    let up = push_snap(&mut m, VK_H, true, true, false).expect("up");
    assert!(!up.consumed);
    let fresh = push_snap(&mut m, VK_H, false, true, false).expect("fresh down");
    assert!(fresh.consumed && fresh.announce);
}

#[test]
fn gate_off_pair_completes_consumed_hold() {
    // Gate on at down, off mid-hold: repeats and the paired up keep the down
    // verdict (swallowed), the mask survives the transition, and a managed
    // origin with the gate off still passes fresh while the gate holds with
    // no origin still swallows.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_H,
            false,
            Some(origin_of(11, "w11")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, false, None, false, tick, false),
        Some(true),
        "consumed repeat stays swallowed across gate-off"
    );
    match q.pop_front().expect("down") {
        QueuedSnapEvent::Intent(i) => assert_eq!((i.op, i.consumed), (SnapOp::Focus, true)),
        _ => panic!("intent"),
    }
    match q.pop_front().expect("repeat") {
        QueuedSnapEvent::Intent(i) => {
            assert!(i.consumed);
            assert!(!i.announce, "disabled handling must not dispatch repeats");
            assert_eq!(i.op, SnapOp::Focus, "repeat pins the down-time op");
        }
        _ => panic!("intent"),
    }
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_H, true, None, false, tick, false),
        Some(true),
        "paired up completes swallowed across gate-off"
    );
    assert!(
        win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick),
        "mask survives the gate transition"
    );
    // Fresh chords while the gate is off pass, even with a managed origin.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_J,
            false,
            Some(origin_of(12, "w12")),
            false,
            tick,
            false
        ),
        Some(false),
        "managed fresh chord passes while gate off"
    );
    // ... while no origin with the gate on still swallows (inactive tiling
    // never clears the gate; the owner settles without acting).
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_J, false, None, false, tick, true),
        Some(true)
    );
}

#[test]
fn owned_hold_modifier_transition_matrix() {
    // Every arm: a consumed hold stays swallowed across Ctrl/Alt/Win/Shift
    // transitions until its matching up, repeats only announce while the
    // live combination still matches the pinned down-time op, and fresh
    // unowned chords still pass through untracked. No timeout recovery: only
    // the matching up closes a hold.
    // Directional focus: Ctrl/Alt/Shift/Win-up transitions swallow silently.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_snap(&mut m, VK_H, false, true, false).expect("focus down");
    assert_eq!(down.op, SnapOp::Focus);
    assert!(down.consumed && down.announce);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    let rep = push_snap(&mut m, VK_H, false, true, false).expect("ctrl repeat");
    assert_eq!(rep.op, SnapOp::Focus);
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_CONTROL, true, true, false);
    push_snap(&mut m, VK_MENU, false, true, false);
    let rep = push_snap(&mut m, VK_H, false, true, false).expect("alt repeat");
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_MENU, true, true, false);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let rep = push_snap(&mut m, VK_H, false, true, false).expect("shift repeat");
    assert_eq!(rep.op, SnapOp::Focus, "armed op stays pinned under Shift");
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    push_snap(&mut m, VK_LWIN, true, true, false);
    let rep = push_snap(&mut m, VK_H, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce, "no dispatch after Win-up");
    let up = push_snap(&mut m, VK_H, true, true, false).expect("paired up");
    assert_eq!(up.op, SnapOp::Focus);
    assert!(up.consumed);
    assert_eq!(push_snap(&mut m, VK_H, true, true, false), None);
    // Directional move: releasing Shift keeps the pinned move op swallowed.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let down = push_snap(&mut m, VK_H, false, true, false).expect("move down");
    assert_eq!(down.op, SnapOp::Move);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let rep = push_snap(&mut m, VK_H, false, true, false).expect("unshift repeat");
    assert_eq!(rep.op, SnapOp::Move);
    assert!(rep.consumed && !rep.announce);
    let up = push_snap(&mut m, VK_H, true, true, false).expect("paired up");
    assert_eq!(up.op, SnapOp::Move);
    assert!(up.consumed);
    // Shift+L to Win+L (default gate): the move hold swallows its repeats
    // without dispatching once Shift leaves, and the up stays swallowed.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let down = push_snap(&mut m, VK_L, false, true, false).expect("shift+l down");
    assert_eq!(down.op, SnapOp::Move);
    assert!(down.consumed);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let rep = push_snap(&mut m, VK_L, false, true, false).expect("win+l repeat");
    assert_eq!(rep.op, SnapOp::Move);
    assert!(
        rep.consumed && !rep.announce,
        "gated Focus+L never dispatches"
    );
    let up = push_snap(&mut m, VK_L, true, true, false).expect("paired up");
    assert!(up.consumed);
    // Default Win+L without Shift still passes through untracked entirely.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_snap(&mut m, VK_L, false, true, false), None);
    assert_eq!(push_snap(&mut m, VK_L, true, true, false), None);
    // Digit select: Shift mid-hold pins select, swallows silently.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    let down = push_workspace(&mut m, VK_0 + 1, false, true, false).expect("select down");
    assert_eq!(down.op, WorkspaceOp::Select);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let rep = push_workspace(&mut m, VK_0 + 1, false, true, false).expect("shift repeat");
    assert_eq!(rep.op, WorkspaceOp::Select);
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let up = push_workspace(&mut m, VK_0 + 1, true, true, false).expect("paired up");
    assert_eq!(up.op, WorkspaceOp::Select);
    assert!(up.consumed);
    // Digit send: releasing Shift pins send, swallows silently.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let down = push_workspace(&mut m, VK_0 + 2, false, true, false).expect("send down");
    assert_eq!(down.op, WorkspaceOp::Send);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let rep = push_workspace(&mut m, VK_0 + 2, false, true, false).expect("unshift repeat");
    assert_eq!(rep.op, WorkspaceOp::Send);
    assert!(rep.consumed && !rep.announce);
    let up = push_workspace(&mut m, VK_0 + 2, true, true, false).expect("paired up");
    assert_eq!(up.op, WorkspaceOp::Send);
    assert!(up.consumed);
    // Maximize: Shift/Ctrl/Win transitions all swallow, never re-dispatch.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(
        push_maximize(&mut m, false, true, false)
            .expect("m down")
            .consumed
    );
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let rep = push_maximize(&mut m, false, true, false).expect("shift repeat");
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    let rep = push_maximize(&mut m, false, true, false).expect("ctrl repeat");
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_CONTROL, true, true, false);
    push_snap(&mut m, VK_LWIN, true, true, false);
    let rep = push_maximize(&mut m, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce);
    assert!(
        push_maximize(&mut m, true, true, false)
            .expect("paired up")
            .consumed
    );
    // Fullscreen: same contract across Shift and Win transitions.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(
        push_fullscreen(&mut m, false, true, false)
            .expect("f11 down")
            .consumed
    );
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let rep = push_fullscreen(&mut m, false, true, false).expect("shift repeat");
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    push_snap(&mut m, VK_LWIN, true, true, false);
    let rep = push_fullscreen(&mut m, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce);
    assert!(
        push_fullscreen(&mut m, true, true, false)
            .expect("paired up")
            .consumed
    );
    // Float: Shift mid-hold never flips into sticky; Win-up keeps swallowing.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(
        push_float(&mut m, false, true, false)
            .expect("g down")
            .consumed
    );
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let rep = push_float(&mut m, false, true, false).expect("shift repeat");
    assert!(rep.consumed && !rep.announce);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    push_snap(&mut m, VK_LWIN, true, true, false);
    let rep = push_float(&mut m, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce);
    assert!(
        push_float(&mut m, true, true, false)
            .expect("paired up")
            .consumed
    );
    assert_eq!((m.sticky_counts.down, m.sticky_counts.up), (0, 0));
    // Sticky: releasing Shift never flips into float.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert!(
        push_sticky(&mut m, false, true, false)
            .expect("sticky down")
            .consumed
    );
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let rep = push_sticky(&mut m, false, true, false).expect("unshift repeat");
    assert!(rep.consumed && !rep.announce);
    let up = push_sticky(&mut m, true, true, false).expect("paired up");
    assert!(up.consumed);
    assert_eq!((m.float_counts.down, m.float_counts.up), (0, 0));
    // Fresh unowned chords still pass through untracked with no hold armed
    // (Alt on a non-catalog key). Ctrl owns the history arm: Win+Ctrl+H
    // classifies history-prev with its pair closing consumed.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_MENU, false, true, false);
    assert_eq!(SnapClassify::push(&mut m, VK_M, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_M, true, true, false), None);
    push_snap(&mut m, VK_MENU, true, true, false);
    {
        use tiler_windows::snapkey::VK_CONTROL;
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        SnapClassify::push(&mut m, VK_CONTROL, false, true, false);
        match SnapClassify::push(&mut m, VK_H, false, true, false).expect("history down") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed && intent.announce),
            other => panic!("expected history, got {other:?}"),
        }
        match SnapClassify::push(&mut m, VK_H, true, true, false).expect("history up") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed),
            other => panic!("expected history up, got {other:?}"),
        }
        SnapClassify::push(&mut m, VK_CONTROL, true, true, false);
    }
    let mut m = SnapClassify::new(takeover());
    assert_eq!(push_snap(&mut m, VK_H, false, true, false), None);
}

#[test]
fn bare_repeat_after_win_up_never_rearms_mask() {
    use tiler_windows::snapkey::{VK_G, VK_M};
    // Every arm: consumed down, Win-up fires the mask once, the bare repeat
    // after Win-up stays swallowed without rearming, the key-up closes the
    // hold, and a later naked Win tap never masks Start.
    let tick = std::time::Instant::now();
    for vk in [VK_H, VK_J, VK_K, VK_LEFT, VK_DOWN, VK_UP, VK_RIGHT] {
        let mut m = SnapClassify::new(takeover());
        let mut q = SnapQueue::new();
        win_down(&mut m, VK_LWIN);
        assert!(
            push_snap(&mut m, vk, false, true, false)
                .expect("down")
                .consumed
        );
        assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
        let rep = push_snap(&mut m, vk, false, true, false).expect("bare repeat");
        assert!(rep.consumed, "vk {vk} bare repeat stays swallowed");
        assert!(
            push_snap(&mut m, vk, true, true, false)
                .expect("up")
                .consumed
        );
        win_down(&mut m, VK_LWIN);
        assert!(
            !win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick),
            "vk {vk} must not rearm the mask for a naked Win tap"
        );
    }
    for index in [1u32, 5, 9] {
        let vk = VK_0 + index;
        let mut m = SnapClassify::new(takeover());
        let mut q = SnapQueue::new();
        win_down(&mut m, VK_LWIN);
        assert!(
            push_workspace(&mut m, vk, false, true, false)
                .expect("down")
                .consumed
        );
        assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
        let rep = push_workspace(&mut m, vk, false, true, false).expect("bare repeat");
        assert!(rep.consumed);
        assert!(
            push_workspace(&mut m, vk, true, true, false)
                .expect("up")
                .consumed
        );
        win_down(&mut m, VK_LWIN);
        assert!(!win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    }
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    assert!(
        push_maximize(&mut m, false, true, false)
            .expect("down")
            .consumed
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    let rep = push_maximize(&mut m, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce);
    assert!(
        push_maximize(&mut m, true, true, false)
            .expect("up")
            .consumed
    );
    win_down(&mut m, VK_LWIN);
    assert!(!win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    assert!(
        push_fullscreen(&mut m, false, true, false)
            .expect("down")
            .consumed
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    let rep = push_fullscreen(&mut m, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce);
    assert!(
        push_fullscreen(&mut m, true, true, false)
            .expect("up")
            .consumed
    );
    win_down(&mut m, VK_LWIN);
    assert!(!win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    assert!(
        push_float(&mut m, false, true, false)
            .expect("down")
            .consumed
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    let rep = push_float(&mut m, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce);
    assert!(push_float(&mut m, true, true, false).expect("up").consumed);
    win_down(&mut m, VK_LWIN);
    assert!(!win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert!(
        push_sticky(&mut m, false, true, false)
            .expect("down")
            .consumed
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let rep = push_sticky(&mut m, false, true, false).expect("bare repeat");
    assert!(rep.consumed && !rep.announce);
    assert!(push_sticky(&mut m, true, true, false).expect("up").consumed);
    win_down(&mut m, VK_LWIN);
    assert!(!win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    assert!(is_chord_vk(VK_M) && is_chord_vk(VK_G));
    // Key-first order still masks exactly once: key-up while Win is held,
    // then Win-up fires.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    assert!(
        push_snap(&mut m, VK_H, false, true, false)
            .expect("down")
            .consumed
    );
    assert!(
        push_snap(&mut m, VK_H, true, true, false)
            .expect("up")
            .consumed
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    assert!(
        push_maximize(&mut m, false, true, false)
            .expect("down")
            .consumed
    );
    assert!(
        push_maximize(&mut m, true, true, false)
            .expect("up")
            .consumed
    );
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
}

#[test]
fn callback_diag_privacy_never_records_typed_letters_without_win() {
    use tiler_windows::snapkey::{
        CALLBACK_DIAG_CAP, CallbackDiag, CallbackDiagBuf, CallbackReason, CallbackSource, VK_MASK,
        callback_diag_allows, is_callback_diag_candidate,
    };
    // Ordinary typing keys are never candidates: no borrow, no record.
    assert!(!is_callback_diag_candidate(0x41));
    assert!(!callback_diag_allows(0x41, false, false, false));
    // Win+B is a launcher chord key, so B is a candidate gated exactly like
    // the other letter catalog keys (typed B without Win still denied).
    assert!(is_callback_diag_candidate(0x42));
    assert!(!callback_diag_allows(0x42, false, false, false));
    assert!(callback_diag_allows(0x42, true, false, false));
    // Letter catalog keys without Win/armed/marked are denied (typed-letter
    // risk); non-letter candidates stay gated the same way.
    assert!(is_callback_diag_candidate(VK_H));
    assert!(!callback_diag_allows(VK_H, false, false, false));
    assert!(callback_diag_allows(VK_H, true, false, false));
    assert!(callback_diag_allows(VK_H, false, true, false));
    assert!(callback_diag_allows(VK_H, false, false, true));
    // Modifiers/Win/E8 are always safe closed vocabulary.
    assert!(callback_diag_allows(VK_SHIFT, false, false, false));
    assert!(callback_diag_allows(VK_LWIN, false, false, false));
    assert!(callback_diag_allows(VK_MASK, false, false, false));
    // Evidence carries closed key vocabulary plus state/timing only: no
    // HWNDs, PIDs, tokens, or titles.
    let diag = CallbackDiag {
        vk: VK_H,
        scan: 35,
        is_up: false,
        source: CallbackSource::Physical,
        reason: CallbackReason::Consumed,
        win_before: true,
        win_after: true,
        shift: false,
        ctrl: false,
        alt: false,
        guard_disagree: false,
        win_untracked: false,
        consumed: true,
        duration_us: 12,
    };
    let ev = tiler_windows::snapkey::callback_diag_evidence(&diag);
    assert_eq!(ev["vk"], serde_json::json!(VK_H));
    assert_eq!(ev["reason"], serde_json::json!("consumed"));
    assert_eq!(ev["duration_us"], serde_json::json!(12));
    assert!(ev.get("hwnd").is_none());
    assert!(ev.get("pid").is_none());
    assert!(ev.get("token").is_none());
    assert!(ev.get("title").is_none());
    // Bounded eviction counts loss; privacy skips count filtered.
    let mut buf = CallbackDiagBuf::new();
    for _ in 0..CALLBACK_DIAG_CAP + 3 {
        buf.push(diag);
    }
    assert_eq!(buf.len(), CALLBACK_DIAG_CAP);
    assert_eq!(buf.dropped, 3);
    buf.note_filtered();
    assert_eq!(buf.filtered, 1);
    assert_eq!(buf.drain().len(), CALLBACK_DIAG_CAP);
    assert!(buf.is_empty());
    // Loss counters saturate instead of wrapping on overflow runs.
    buf.dropped = u32::MAX;
    buf.push(diag);
    buf.push(diag);
    assert_eq!(buf.dropped, u32::MAX);
    buf.filtered = u32::MAX;
    buf.note_filtered();
    assert_eq!(buf.filtered, u32::MAX);
}

#[test]
fn callback_win_untracked_covers_physical_and_proof_paths() {
    use tiler_windows::snapkey::{VK_H, VK_LWIN, callback_win_untracked};
    // Missing tracked Win is relevant on either path: proof-marking never
    // supplies the missing Win edge, so no marked exemption.
    assert!(callback_win_untracked(VK_H, false));
    assert!(!callback_win_untracked(VK_H, true));
    // Non-chord vocabulary never flags, even without Win.
    assert!(!callback_win_untracked(VK_LWIN, false));
    assert!(!callback_win_untracked(0x41, false));
}

#[test]
fn callback_diag_reason_vocabulary_covers_pass_through() {
    use tiler_windows::snapkey::{CallbackReason, CallbackSource};
    assert_eq!(CallbackSource::Physical.as_str(), "physical");
    assert_eq!(CallbackSource::ProofMarked.as_str(), "proof-marked");
    assert_eq!(
        CallbackSource::InjectedFiltered.as_str(),
        "injected-filtered"
    );
    for (reason, want) in [
        (CallbackReason::InjectedFiltered, "injected-filtered"),
        (CallbackReason::ModifierGuard, "modifier-guard"),
        (CallbackReason::Unclassified, "unclassified"),
        (CallbackReason::Consumed, "consumed"),
        (CallbackReason::Passed, "passed"),
        (CallbackReason::WinUpPass, "win-up-pass"),
    ] {
        assert_eq!(reason.as_str(), want);
    }
}

// Item 2: explicit follow/stay routing for numbered and relative sends.
// Windows Ctrl+Shift arrow ownership is UNKNOWN: defaults keep pending
// evidenced conflicts (no new Compatible disables), stay rows ride explicit
// actions with no canonical default VK.

fn push_send(
    m: &mut SnapClassify,
    vk: u32,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::WorkspaceSendIntent> {
    match SnapClassify::push(m, vk, is_up, fg, inj)? {
        Classified::WorkspaceSend(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected relative send")
        }
    }
}

fn ctrl_shift_down(m: &mut SnapClassify) {
    push_snap(m, VK_SHIFT, false, true, false);
    push_snap(m, VK_CONTROL, false, true, false);
}

#[test]
fn relative_follow_direct_chords_route_all_eight_keys() {
    // Win+Ctrl+Shift+H/K/Left/Up step previous (-1, follow);
    // J/L/Down/Right step next (+1, follow). Tab never sends.
    for (vk, delta) in [
        (VK_H, -1),
        (VK_K, -1),
        (VK_LEFT, -1),
        (VK_UP, -1),
        (VK_J, 1),
        (VK_L, 1),
        (VK_DOWN, 1),
        (VK_RIGHT, 1),
    ] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        ctrl_shift_down(&mut m);
        let down = push_send(&mut m, vk, false, true, false).expect("relative down");
        assert_eq!((down.delta, down.follow), (delta, true));
        assert!(down.consumed && down.announce);
        let repeat = push_send(&mut m, vk, false, true, false).expect("repeat");
        assert_eq!(repeat.edge, SnapEdge::Repeat);
        assert!(repeat.consumed && repeat.announce);
        let up = push_send(&mut m, vk, true, true, false).expect("paired up");
        assert!(up.consumed && !up.announce);
        assert!(!m.key_is_down(vk));
    }
    // Tab with Ctrl+Shift is not a send arm.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    ctrl_shift_down(&mut m);
    assert_eq!(
        SnapClassify::push(&mut m, tiler_windows::snapkey::VK_TAB, false, true, false),
        None
    );
}

#[test]
fn relative_send_repeat_gates_on_live_modifiers() {
    // Shift released mid-hold: the repeat stays swallowed without dispatch,
    // and the paired up still closes consumed.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    ctrl_shift_down(&mut m);
    let down = push_send(&mut m, VK_J, false, true, false).expect("down");
    assert!(down.consumed && down.announce);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let repeat = push_send(&mut m, VK_J, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    let up = push_send(&mut m, VK_J, true, true, false).expect("up");
    assert!(up.consumed && !up.announce);
}

#[test]
fn numbered_send_direct_chord_follows_and_stay_needs_its_action() {
    // Direct Win+Shift+digit always follows; the unbound stay rows never fire
    // without their explicit rebind action.
    for index in [0u8, 1, 5, 9] {
        let vk = VK_0 + u32::from(index);
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, VK_SHIFT, false, true, false);
        let send = push_workspace(&mut m, vk, false, true, false).expect("send down");
        assert_eq!((send.op, send.index), (WorkspaceOp::Send, index));
        assert!(send.follow, "direct numbered sends follow");
        assert!(send.consumed && send.announce);
    }
}

#[test]
fn numbered_stay_rebind_routes_stay_with_shift_arm() {
    // stay-workspace-2 rebound to Win+Shift+F6 routes stay (follow=false);
    // the direct Win+Shift+2 chord still follows (stay rows carry no default
    // to suppress).
    use tiler_windows::settings::{
        BindingSetting, BindingState, Settings, build_disabled, build_remap, validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-2".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Shift+F6".to_owned()),
        },
    );
    validate_settings(&settings).expect("stay rebind validates");
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(remap[0].action, ChordAction::WorkspaceStayDigit);
    let mut m = SnapClassify::new(takeover());
    m.set_remap(remap);
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let stay = push_workspace(&mut m, 0x75, false, true, false).expect("stay down");
    assert_eq!((stay.op, stay.index), (WorkspaceOp::Send, 2));
    assert!(!stay.follow, "stay action pins follow=false");
    assert!(stay.consumed && stay.announce);
    let up = push_workspace(&mut m, 0x75, true, true, false).expect("stay up");
    assert!(!up.follow && up.consumed);
    // Direct chord unaffected: still follow.
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&settings));
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let direct = push_workspace(&mut m, VK_0 + 2, false, true, false).expect("direct");
    assert!(direct.follow);
}

fn settings_default_with_ctrl_shift_stay() -> tiler_windows::settings::Settings {
    use tiler_windows::settings::{BindingSetting, BindingState, Settings};
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-2".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    settings
}

#[test]
fn numbered_stay_ctrl_shift_arm_routes_stay() {
    // Backlog journey example: stay-workspace-2 rebound to Win+Ctrl+Shift+F6
    // routes stay (follow=false) with the pinned Ctrl arm; the plain
    // Win+Shift stay rebind capability is untouched (see the previous test).
    use tiler_windows::settings::{
        BindingSetting, BindingState, Settings, build_disabled, build_remap, validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-2".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    validate_settings(&settings).expect("ctrl+shift stay rebind validates");
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(
        (
            remap[0].from_vk,
            remap[0].from_shift,
            remap[0].from_ctrl,
            remap[0].from_alt,
            remap[0].action,
            remap[0].to_vk,
        ),
        (
            0x75,
            true,
            true,
            false,
            ChordAction::WorkspaceStayDigit,
            VK_0 + 2
        )
    );
    let mut m = SnapClassify::new(takeover());
    m.set_remap(remap);
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    ctrl_shift_down(&mut m);
    let stay = push_workspace(&mut m, 0x75, false, true, false).expect("stay down");
    assert_eq!((stay.op, stay.index), (WorkspaceOp::Send, 2));
    assert!(!stay.follow, "stay action pins follow=false");
    assert!(stay.consumed && stay.announce);
    // Repeat dispatches while the pinned Win+Ctrl+Shift arm holds.
    let repeat = push_workspace(&mut m, 0x75, false, true, false).expect("stay repeat");
    assert!(!repeat.follow && repeat.consumed && repeat.announce);
    // Ctrl released mid-hold: the repeat swallows (consumed, no dispatch),
    // leaking no repeat and never flipping into follow.
    push_snap(&mut m, VK_CONTROL, true, true, false);
    let swallowed = push_workspace(&mut m, 0x75, false, true, false).expect("swallowed repeat");
    assert!(!swallowed.follow && swallowed.consumed && !swallowed.announce);
    // Paired up still closes consumed with the pinned stay identity.
    let up = push_workspace(&mut m, 0x75, true, true, false).expect("stay up");
    assert_eq!((up.op, up.index, up.follow), (WorkspaceOp::Send, 2, false));
    assert!(up.consumed && !up.announce);
    // Wrong modifiers pass through untracked with their pairs: Win+Shift+F6
    // without Ctrl is not the rebound chord, and Alt never arms stay.
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&settings));
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    assert_eq!(push_workspace(&mut m, 0x75, false, true, false), None);
    assert_eq!(push_workspace(&mut m, 0x75, true, true, false), None);
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&settings));
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    push_snap(&mut m, VK_MENU, false, true, false);
    assert_eq!(push_workspace(&mut m, 0x75, false, true, false), None);
    assert_eq!(push_workspace(&mut m, 0x75, true, true, false), None);
    // Rebind-away pins remain: the direct Win+Shift+2 chord still follows
    // (stay rows carry no default to suppress).
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&settings));
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let direct = push_workspace(&mut m, VK_0 + 2, false, true, false).expect("direct");
    assert_eq!((direct.op, direct.follow), (WorkspaceOp::Send, true));
    // Disabled stay passes the rebound chord through entirely.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "stay-workspace-2".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    let mut m = SnapClassify::new(takeover());
    m.set_remap(build_remap(&settings));
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    ctrl_shift_down(&mut m);
    assert_eq!(push_workspace(&mut m, 0x75, false, true, false), None);
    assert_eq!(push_workspace(&mut m, 0x75, true, true, false), None);
    // A consumed Ctrl+Shift stay arms the Start-menu mask at Win-up.
    use tiler_windows::snapkey::MaskTrigger;
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_SHIFT, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_CONTROL, false, None, false, tick, true),
        None
    );
    m.set_remap(build_remap(&settings_default_with_ctrl_shift_stay()));
    m.set_disabled(build_disabled(&settings_default_with_ctrl_shift_stay()));
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            0x75,
            false,
            Some(origin_of(7, "w7")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    match q.pop_front().expect("stay intent") {
        QueuedSnapEvent::Workspace(intent) => {
            assert_eq!(
                (intent.op, intent.index, intent.follow),
                (WorkspaceOp::Send, 2, false)
            );
            assert!(intent.consumed && intent.announce);
        }
        _ => panic!("expected workspace intent"),
    }
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => match mask.trigger {
            MaskTrigger::Workspace { op, index } => {
                assert_eq!((op, index), (WorkspaceOp::Send, 2));
            }
            _ => panic!("expected workspace mask trigger"),
        },
        _ => panic!("expected mask"),
    }
}

#[test]
fn relative_follow_rebind_away_passes_old_default_through() {
    // send-prev-h rebound to Win+Ctrl+Shift+F6: the new chord follows prev,
    // the rebound-away Win+Ctrl+Shift+H passes through with its pair.
    use tiler_windows::settings::{
        BindingSetting, BindingState, Settings, build_disabled, build_remap, validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let mut settings = Settings::default();
    settings.bindings.insert(
        "send-prev-h".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    validate_settings(&settings).expect("relative rebind validates");
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(remap[0].action, ChordAction::WorkspaceSendPrev);
    let mut m = SnapClassify::new(takeover());
    m.set_remap(remap);
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    ctrl_shift_down(&mut m);
    let rebound = push_send(&mut m, 0x75, false, true, false).expect("rebound");
    assert_eq!((rebound.delta, rebound.follow), (-1, true));
    assert!(rebound.consumed && rebound.announce);
    // Rebound-away canonical passes through on a fresh machine.
    let mut plain = SnapClassify::new(takeover());
    plain.set_remap(build_remap(&settings));
    plain.set_disabled(build_disabled(&settings));
    win_down(&mut plain, VK_LWIN);
    ctrl_shift_down(&mut plain);
    assert_eq!(
        SnapClassify::push(&mut plain, VK_H, false, true, false),
        None
    );
    assert_eq!(
        SnapClassify::push(&mut plain, VK_H, true, true, false),
        None
    );
}

#[test]
fn relative_stay_rebind_routes_stay_without_touching_follow() {
    // send-stay-next-j rebound to Win+Ctrl+Shift+F6 routes stay-next; the
    // direct Win+Ctrl+Shift+J follow still fires on a live machine.
    use tiler_windows::settings::{
        BindingSetting, BindingState, Settings, build_disabled, build_remap, validate_settings,
    };
    use tiler_windows::snapkey::ChordAction;
    let mut settings = Settings::default();
    settings.bindings.insert(
        "send-stay-next-j".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+Shift+F6".to_owned()),
        },
    );
    validate_settings(&settings).expect("stay rebind validates");
    let remap = build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(remap[0].action, ChordAction::WorkspaceSendStayNext);
    let mut m = SnapClassify::new(takeover());
    m.set_remap(remap);
    m.set_disabled(build_disabled(&settings));
    win_down(&mut m, VK_LWIN);
    ctrl_shift_down(&mut m);
    let stay = push_send(&mut m, 0x75, false, true, false).expect("stay");
    assert_eq!((stay.delta, stay.follow), (1, false));
    assert!(stay.consumed && stay.announce);
    // Same canonical slot (J) is held by the stay rebind: a concurrent
    // direct press collides safely (swallowed, pinned stay shape, never
    // dispatched) until the hold closes.
    let collide = push_send(&mut m, VK_J, false, true, false).expect("collide");
    assert!(!collide.follow && collide.consumed && !collide.announce);
    let up = push_send(&mut m, VK_J, true, true, false).expect("collide up");
    assert!(up.consumed && !up.announce);
    let stay_up = push_send(&mut m, 0x75, true, true, false).expect("stay up");
    assert!(stay_up.consumed && !stay_up.announce);
    let direct = push_send(&mut m, VK_J, false, true, false).expect("direct follow");
    assert_eq!((direct.delta, direct.follow), (1, true));
    assert!(direct.consumed && direct.announce);
}

#[test]
fn relative_send_queue_carries_delta_follow_and_masks() {
    // The queued record pins delta/follow/origin for the owner; a consumed
    // relative send arms the Start-menu mask at Win-up like digits.
    use tiler_windows::snapkey::{MaskTrigger, QueuedWorkspaceSendIntent};
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_SHIFT, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_CONTROL, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_J,
            false,
            Some(origin_of(9, "w9")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    match q.pop_front().expect("relative intent") {
        QueuedSnapEvent::WorkspaceSend(QueuedWorkspaceSendIntent {
            delta,
            follow,
            origin,
            ..
        }) => {
            assert_eq!((delta, follow), (1, true));
            assert_eq!(origin, Some(origin_of(9, "w9")));
        }
        other => panic!("expected relative send, got {other:?}"),
    }
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => match mask.trigger {
            MaskTrigger::WorkspaceSend { delta, follow } => {
                assert_eq!((delta, follow), (1, true));
            }
            _ => panic!("expected relative send mask trigger"),
        },
        _ => panic!("expected mask"),
    }
}

fn push_resize(
    m: &mut SnapClassify,
    vk: u32,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::ResizeIntent> {
    match SnapClassify::push(m, vk, is_up, fg, inj)? {
        Classified::Resize(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Launch(_) => {
            panic!("expected resize chord")
        }
    }
}

fn alt_down(m: &mut SnapClassify) {
    push_snap(m, VK_MENU, false, true, false);
}

fn alt_up(m: &mut SnapClassify) {
    push_snap(m, VK_MENU, true, true, false);
}

// Keyboard resize: all 16 physical chords (8 logical rows, letter plus arrow
// alias each) route to dedicated resize intents with the exact mode, never a
// directional move.
#[test]
fn resize_exact_modifiers_mode_and_aliases() {
    use tiler_windows::snapkey::ResizeMode;
    let cases: &[(u32, u32, Direction)] = &[
        (VK_H, VK_LEFT, Direction::Left),
        (VK_J, VK_DOWN, Direction::Down),
        (VK_K, VK_UP, Direction::Up),
        (VK_L, VK_RIGHT, Direction::Right),
    ];
    for (letter, arrow, direction) in cases {
        for vk in [*letter, *arrow] {
            // Outwards: Win+Alt+key.
            let mut m = SnapClassify::new(takeover());
            win_down(&mut m, VK_LWIN);
            alt_down(&mut m);
            let down = push_resize(&mut m, vk, false, true, false).expect("out down");
            assert_eq!(down.mode, ResizeMode::Outwards);
            assert_eq!(down.direction, *direction);
            assert_eq!(down.edge, SnapEdge::Down);
            assert!(down.consumed && down.announce);
            let up = push_resize(&mut m, vk, true, true, false).expect("out up");
            assert_eq!(
                (up.mode, up.edge, up.consumed),
                (ResizeMode::Outwards, SnapEdge::Up, true)
            );
            // Inwards: Win+Shift+Alt+key.
            let mut m = SnapClassify::new(takeover());
            win_down(&mut m, VK_LWIN);
            push_snap(&mut m, VK_SHIFT, false, true, false);
            alt_down(&mut m);
            let down = push_resize(&mut m, vk, false, true, false).expect("in down");
            assert_eq!(
                (down.mode, down.direction),
                (ResizeMode::Inwards, *direction)
            );
            assert!(down.consumed && down.announce);
            // Releasing Shift before the key-up keeps the down-time mode.
            push_snap(&mut m, VK_SHIFT, true, true, false);
            let up = push_resize(&mut m, vk, true, true, false).expect("in up");
            assert_eq!((up.mode, up.consumed), (ResizeMode::Inwards, true));
            alt_up(&mut m);
        }
    }
}

// Resize repeat dispatch stays pinned to the required modifiers/action: only
// a live Win+Alt(+Shift) combination matching the down-time mode dispatches;
// anything else swallows without dispatching but keeps the pair consumed.
#[test]
fn resize_repeat_pins_mode_modifiers_and_action() {
    use tiler_windows::snapkey::ResizeMode;
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    alt_down(&mut m);
    let down = push_resize(&mut m, VK_H, false, true, false).expect("down");
    assert_eq!(down.mode, ResizeMode::Outwards);
    // Live repeat dispatches.
    let repeat = push_resize(&mut m, VK_H, false, true, false).expect("repeat");
    assert_eq!(
        (repeat.mode, repeat.edge),
        (ResizeMode::Outwards, SnapEdge::Repeat)
    );
    assert!(repeat.consumed && repeat.announce);
    // Shift flip (would select inwards) swallows without dispatching and
    // never rewrites the pinned mode.
    push_snap(&mut m, VK_SHIFT, false, true, false);
    let repeat = push_resize(&mut m, VK_H, false, true, false).expect("repeat");
    assert_eq!(repeat.mode, ResizeMode::Outwards);
    assert!(repeat.consumed && !repeat.announce);
    push_snap(&mut m, VK_SHIFT, true, true, false);
    // Extra Ctrl swallows without dispatching.
    push_snap(&mut m, VK_CONTROL, false, true, false);
    let repeat = push_resize(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    push_snap(&mut m, VK_CONTROL, true, true, false);
    // Released Alt swallows without dispatching.
    alt_up(&mut m);
    let repeat = push_resize(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    // Pair still closes consumed.
    let up = push_resize(&mut m, VK_H, true, true, false).expect("up");
    assert!(up.consumed && !up.announce);
    assert_eq!(push_resize(&mut m, VK_H, true, true, false), None);
}

// Unknown extra Ctrl (or any other unbound Alt shape) passes through
// untracked unless an exact effective rebind claims it; bare and injected
// chords never classify. Win+Ctrl+Alt+H is the documented item 5 output
// follow arm (not a resize): it dispatches output, never resize.
#[test]
fn resize_unknown_modifiers_pass_untracked() {
    // Win+Ctrl+Alt+H dispatches the output follow arm, leaving resize
    // counts untouched.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    alt_down(&mut m);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("output") {
        Classified::WorkspaceOutput(intent) => {
            assert_eq!(intent.direction, Direction::Left);
            assert!(intent.follow && intent.consumed && intent.announce);
        }
        other => panic!("expected output send, got {other:?}"),
    }
    assert_eq!((m.resize_counts[0].down, m.resize_counts[0].up), (0, 0));
    assert_eq!(
        (m.output_counts[0].down, m.output_counts[0].consumed),
        (1, 1)
    );
    // Bare Alt+H without Win passes; injected resize never classifies.
    let mut m = SnapClassify::new(takeover());
    push_snap(&mut m, VK_MENU, false, true, false);
    assert_eq!(SnapClassify::push(&mut m, VK_H, false, true, false), None);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    alt_down(&mut m);
    assert_eq!(push_resize(&mut m, VK_H, false, true, true), None);
    assert!(!win_up_mask_reserve(
        &mut m,
        &mut SnapQueue::new(),
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
}

// Held resize survives settings transitions: removing the remap or
// disabling the row mid-hold gates repeat dispatch but keeps the consumed
// pair; fresh presses afterwards pass through.
#[test]
fn resize_held_rebind_disable_gate_repeat_but_keep_pair() {
    use tiler_windows::snapkey::{ChordAction, ChordDisable, ChordRemap};
    let remap = || {
        vec![ChordRemap {
            from_vk: 0x55,
            from_shift: false,
            from_ctrl: false,
            from_alt: true,
            action: ChordAction::ResizeOut,
            to_vk: VK_H,
        }]
    };
    let mut m = SnapClassify::new(takeover());
    m.set_remap(remap());
    win_down(&mut m, VK_LWIN);
    alt_down(&mut m);
    match SnapClassify::push(&mut m, 0x55, false, true, false).expect("rebound down") {
        Classified::Resize(intent) => {
            assert_eq!(intent.mode, tiler_windows::snapkey::ResizeMode::Outwards);
            assert!(intent.consumed && intent.announce);
        }
        other => panic!("expected resize, got {other:?}"),
    }
    m.set_remap(Vec::new());
    match SnapClassify::push(&mut m, 0x55, false, true, false).expect("repeat") {
        Classified::Resize(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected swallowed repeat, got {other:?}"),
    }
    match SnapClassify::push(&mut m, 0x55, true, true, false).expect("up") {
        Classified::Resize(intent) => assert!(intent.consumed),
        other => panic!("expected up, got {other:?}"),
    }
    assert_eq!(SnapClassify::push(&mut m, 0x55, false, true, false), None);
    // Canonical disable mid-hold gates dispatch but keeps the pair.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    alt_down(&mut m);
    push_resize(&mut m, VK_H, false, true, false).expect("down");
    m.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: true,
    }]);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("repeat") {
        Classified::Resize(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected swallowed repeat, got {other:?}"),
    }
    match SnapClassify::push(&mut m, VK_H, true, true, false).expect("up") {
        Classified::Resize(intent) => assert!(intent.consumed),
        other => panic!("expected up, got {other:?}"),
    }
}

// Resize queues with mode/direction/origin, arms the Start-menu mask at
// Win-up, and saturates fail-closed (still swallows, drops only evidence).
#[test]
fn resize_queue_carries_mode_and_masks_with_saturation() {
    use tiler_windows::snapkey::{MaskTrigger, QueuedResizeIntent, ResizeMode};
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_MENU, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_J,
            false,
            Some(origin_of(5, "w5")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    match q.pop_front().expect("resize intent") {
        QueuedSnapEvent::Resize(QueuedResizeIntent {
            mode,
            direction,
            origin,
            ..
        }) => {
            assert_eq!(mode, ResizeMode::Outwards);
            assert_eq!(direction, Direction::Down);
            assert_eq!(origin, Some(origin_of(5, "w5")));
        }
        other => panic!("expected resize, got {other:?}"),
    }
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => match mask.trigger {
            MaskTrigger::Resize { mode, direction } => {
                assert_eq!((mode, direction), (ResizeMode::Outwards, Direction::Down));
            }
            _ => panic!("expected resize mask trigger"),
        },
        other => panic!("expected mask, got {other:?}"),
    }
    // Saturation: an owned resize still swallows and counts the loss.
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    win_down(&mut m, VK_LWIN);
    alt_down(&mut m);
    while q.len() < INTENT_QUEUE_CAP {
        assert!(q.push(QueuedSnapEvent::Resize(QueuedResizeIntent {
            mode: ResizeMode::Outwards,
            direction: Direction::Left,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick,
        })));
    }
    assert!(q.is_full());
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_H,
            false,
            Some(origin_of(6, "w6")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    assert!(q.dropped >= 1);
}

fn push_launch(
    m: &mut SnapClassify,
    vk: u32,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::LaunchIntent> {
    match SnapClassify::push(m, vk, is_up, fg, inj)? {
        Classified::Launch(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected launcher chord")
        }
    }
}

#[test]
fn launch_browser_consumes_down_repeat_up() {
    // Win+B: down consumes with announce for the browser kind, held repeat
    // is swallowed (no re-spawn), up closes the pair without announce.
    use tiler_windows::snapkey::{LaunchKind, VK_B, is_launch_browser_vk};
    assert!(is_chord_vk(VK_B));
    assert!(is_launch_browser_vk(VK_B));
    assert_eq!(VK_B, 0x42);
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    assert!(!m.key_is_down(VK_B));
    let down = push_launch(&mut m, VK_B, false, true, false).expect("browser down");
    assert_eq!(down.kind, LaunchKind::Browser);
    assert_eq!(down.edge, SnapEdge::Down);
    assert!(down.consumed && down.announce);
    assert!(m.key_is_down(VK_B));
    let repeat = push_launch(&mut m, VK_B, false, true, false).expect("browser repeat");
    assert_eq!(repeat.edge, SnapEdge::Repeat);
    assert!(repeat.consumed && !repeat.announce);
    let up = push_launch(&mut m, VK_B, true, true, false).expect("paired up");
    assert_eq!(up.edge, SnapEdge::Up);
    assert!(up.consumed && !up.announce);
    assert!(!m.key_is_down(VK_B));
    assert_eq!(push_launch(&mut m, VK_B, true, true, false), None);
    assert_eq!(
        (
            m.launch_browser_counts.down,
            m.launch_browser_counts.repeat,
            m.launch_browser_counts.up
        ),
        (1, 1, 1)
    );
}

#[test]
fn launch_terminal_consumes_down_with_kind() {
    // Win+T dispatches the terminal kind through the queue with the
    // chord-time origin riding for mask/queue parity.
    use tiler_windows::snapkey::{LaunchKind, QueuedLaunchIntent, VK_T, is_launch_terminal_vk};
    assert!(is_chord_vk(VK_T));
    assert!(is_launch_terminal_vk(VK_T));
    assert_eq!(VK_T, 0x54);
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_T,
            false,
            Some(origin_of(9, "w9")),
            false,
            tick,
            true
        ),
        Some(true)
    );
    match q.pop_front().expect("launch intent") {
        QueuedSnapEvent::Launch(QueuedLaunchIntent { kind, origin, .. }) => {
            assert_eq!(kind, LaunchKind::Terminal);
            assert_eq!(origin, Some(origin_of(9, "w9")));
        }
        _ => panic!("expected launch intent"),
    }
}

#[test]
fn launch_modifier_exactness_passes_untracked() {
    // Any Shift/Ctrl/Alt, missing Win, bare chords, and injected chords never
    // classify and never arm the mask: there is no shifted launcher arm.
    use tiler_windows::snapkey::{SnapQueue, VK_B, VK_T};
    for vk in [VK_B, VK_T] {
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        push_snap(&mut m, VK_SHIFT, false, true, false);
        assert_eq!(push_launch(&mut m, vk, false, true, false), None);
        assert_eq!(push_launch(&mut m, vk, true, true, false), None);
        push_snap(&mut m, VK_SHIFT, true, true, false);
        for mod_vk in [VK_CONTROL, VK_MENU, VK_LMENU] {
            let mut m = SnapClassify::new(takeover());
            win_down(&mut m, VK_LWIN);
            push_snap(&mut m, mod_vk, false, true, false);
            assert_eq!(push_launch(&mut m, vk, false, true, false), None);
            assert_eq!(push_launch(&mut m, vk, true, true, false), None);
            push_snap(&mut m, mod_vk, true, true, false);
        }
        let mut m = SnapClassify::new(takeover());
        assert_eq!(push_launch(&mut m, vk, false, true, false), None);
        assert_eq!(push_launch(&mut m, vk, false, true, true), None);
        let mut m = SnapClassify::new(takeover());
        win_down(&mut m, VK_LWIN);
        assert_eq!(push_launch(&mut m, vk, false, true, true), None);
        assert!(!win_up_mask_reserve(
            &mut m,
            &mut SnapQueue::new(),
            VK_LWIN,
            true,
            std::time::Instant::now()
        ));
    }
}

#[test]
fn launch_arms_start_menu_mask() {
    // A consumed Win+B / Win+T reserves the E8 mask at Win-up with per-kind
    // trigger evidence.
    use tiler_windows::snapkey::{MaskTrigger, QueuedLaunchIntent, SnapQueue, VK_B, VK_T};
    for (vk, trigger) in [
        (VK_B, MaskTrigger::LaunchBrowser),
        (VK_T, MaskTrigger::LaunchTerminal),
    ] {
        let mut m = SnapClassify::new(takeover());
        let mut q = SnapQueue::new();
        let tick = std::time::Instant::now();
        assert_eq!(
            classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
            None
        );
        assert_eq!(
            classify_and_queue(
                &mut m,
                &mut q,
                vk,
                false,
                Some(origin_of(7, "w7")),
                false,
                tick,
                true
            ),
            Some(true)
        );
        assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
        match q.pop_front().expect("launch intent") {
            QueuedSnapEvent::Launch(QueuedLaunchIntent { origin, .. }) => {
                assert_eq!(origin, Some(origin_of(7, "w7")));
            }
            _ => panic!("expected launch intent"),
        }
        match q.pop_front().expect("mask") {
            QueuedSnapEvent::Mask(mask) => assert_eq!(mask.trigger, trigger),
            _ => panic!("expected mask"),
        }
    }
}

#[test]
fn launch_rebind_and_disable_flow() {
    // Rebound launcher (Win+U -> browser/B) dispatches through the rebound
    // table; disabling the canonical Win+B suppresses fresh downs while the
    // rebound still routes (rebound wins over suppression).
    use tiler_windows::snapkey::{
        ChordAction, ChordDisable, ChordRemap, LaunchKind, VK_B, is_launch_browser_vk,
    };
    const VK_U: u32 = 0x55;
    assert!(is_launch_browser_vk(VK_B));
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: ChordAction::LaunchBrowser,
        to_vk: VK_B,
    }]);
    win_down(&mut m, VK_LWIN);
    match SnapClassify::push(&mut m, VK_U, false, true, false).expect("rebound down") {
        Classified::Launch(intent) => {
            assert_eq!(intent.kind, LaunchKind::Browser);
            assert!(intent.consumed && intent.announce);
        }
        other => panic!("expected launch, got {other:?}"),
    }
    // Disabled canonical passes fresh downs through; the rebound still routes.
    let mut m = SnapClassify::new(takeover());
    m.set_disabled(vec![ChordDisable {
        vk: VK_B,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    win_down(&mut m, VK_LWIN);
    assert_eq!(push_launch(&mut m, VK_B, false, true, false), None);
    assert_eq!(push_launch(&mut m, VK_B, true, true, false), None);
}

// Item 5 explicit output send: Win+Ctrl+Alt over the directional catalog
// (letters plus arrow aliases) dispatches follow intents carrying the
// direction; stay rides only the explicit stay action through the rebound
// table (unbound by default). Repeats/hold/suppression/mask ride the same
// plumbing as the item 1/2 arms.
fn push_output(
    m: &mut SnapClassify,
    vk: u32,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::WorkspaceOutputIntent> {
    match SnapClassify::push(m, vk, is_up, fg, inj)? {
        Classified::WorkspaceOutput(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceMigrate(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected output send")
        }
    }
}

fn ctrl_alt_down(m: &mut SnapClassify) {
    push_snap(m, VK_CONTROL, false, true, false);
    alt_down(m);
}

#[test]
fn output_follow_direct_chords_route_all_eight_keys() {
    // All eight follow defaults (letters plus arrow aliases) dispatch with
    // the exact direction and follow=true; ups close the pair consumed.
    let cases: &[(u32, u32, Direction)] = &[
        (VK_H, VK_LEFT, Direction::Left),
        (VK_J, VK_DOWN, Direction::Down),
        (VK_K, VK_UP, Direction::Up),
        (VK_L, VK_RIGHT, Direction::Right),
    ];
    for (letter, arrow, direction) in cases {
        for vk in [*letter, *arrow] {
            let mut m = SnapClassify::new(takeover());
            win_down(&mut m, VK_LWIN);
            ctrl_alt_down(&mut m);
            let down = push_output(&mut m, vk, false, true, false).expect("down");
            assert_eq!(down.direction, *direction);
            assert!(down.follow, "direct chords follow");
            assert_eq!(down.edge, SnapEdge::Down);
            assert!(down.consumed && down.announce);
            let up = push_output(&mut m, vk, true, true, false).expect("up");
            assert_eq!(
                (up.direction, up.follow, up.edge),
                (*direction, true, SnapEdge::Up)
            );
            assert!(up.consumed && !up.announce);
        }
    }
}

#[test]
fn output_shifted_direct_chord_refuses_without_pin() {
    // Win+Ctrl+Alt+Shift+H refuses with no pin (only an explicit stay rebind
    // routes shifted chords), so the paired key-up passes through as well.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    ctrl_alt_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_H, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_H, true, true, false), None);
}

#[test]
fn output_modifier_arms_do_not_cross() {
    // Each modifier combination on one catalog key dispatches exactly its own
    // arm: directional, history, resize, relative send and output send never
    // leak into each other.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("focus") {
        Classified::Snap(intent) => assert_eq!(intent.op, SnapOp::Focus),
        other => panic!("expected focus, got {other:?}"),
    }
    // Same physical is pinned to focus: the Ctrl-modified press is the
    // owner's swallowed repeat, never a history dispatch.
    push_snap(&mut m, VK_CONTROL, false, true, false);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("repeat") {
        Classified::Snap(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected pinned focus repeat, got {other:?}"),
    }
    // Fresh arms, one modifier shape each.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("history") {
        Classified::WorkspaceHistory(_) => {}
        other => panic!("expected history, got {other:?}"),
    }
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    alt_down(&mut m);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("resize") {
        Classified::Resize(_) => {}
        other => panic!("expected resize, got {other:?}"),
    }
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("send") {
        Classified::WorkspaceSend(intent) => assert!(intent.follow),
        other => panic!("expected relative send, got {other:?}"),
    }
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    ctrl_alt_down(&mut m);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("output") {
        Classified::WorkspaceOutput(intent) => {
            assert_eq!(intent.direction, Direction::Left);
            assert!(intent.follow);
        }
        other => panic!("expected output send, got {other:?}"),
    }
}

#[test]
fn output_stay_rebind_routes_unshifted_and_shifted() {
    // A stay row rebound without Shift (Win+Ctrl+Alt+F6) dispatches stay;
    // a stay row rebound with Shift pins Shift for repeat gating.
    use tiler_windows::snapkey::{ChordAction, ChordRemap};
    const VK_F6: u32 = 0x75;
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_F6,
        from_shift: false,
        from_ctrl: true,
        from_alt: true,
        action: ChordAction::WorkspaceOutputStay,
        to_vk: VK_L,
    }]);
    win_down(&mut m, VK_LWIN);
    ctrl_alt_down(&mut m);
    let down = push_output(&mut m, VK_F6, false, true, false).expect("stay down");
    assert_eq!(down.direction, Direction::Right);
    assert!(!down.follow, "stay pins follow=false");
    assert!(down.consumed && down.announce);
    let up = push_output(&mut m, VK_F6, true, true, false).expect("stay up");
    assert!(!up.follow && up.consumed);
    // Shifted stay rebind dispatches stay and gates repeats on Shift.
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_F6,
        from_shift: true,
        from_ctrl: true,
        from_alt: true,
        action: ChordAction::WorkspaceOutputStay,
        to_vk: VK_L,
    }]);
    win_down(&mut m, VK_LWIN);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    ctrl_alt_down(&mut m);
    let down = push_output(&mut m, VK_F6, false, true, false).expect("shifted stay");
    assert!(!down.follow && down.consumed && down.announce);
    // Releasing Shift mid-hold swallows the repeat without dispatching.
    push_snap(&mut m, VK_SHIFT, true, true, false);
    let repeat = push_output(&mut m, VK_F6, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    let up = push_output(&mut m, VK_F6, true, true, false).expect("up");
    assert!(up.consumed);
}

#[test]
fn output_repeat_rides_pinned_arm_and_suppression() {
    // Held repeats dispatch while the live Win+Ctrl+Alt combination still
    // matches the pinned arm; a mid-hold modifier flip or suppression
    // swallows without dispatching while the pair still closes consumed.
    use tiler_windows::snapkey::ChordDisable;
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    ctrl_alt_down(&mut m);
    let down = push_output(&mut m, VK_H, false, true, false).expect("down");
    assert!(down.consumed && down.announce);
    let repeat = push_output(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed && repeat.announce);
    assert_eq!(repeat.direction, Direction::Left);
    assert!(repeat.follow, "follow rides the pinned intent");
    // Mid-hold Alt release swallows without dispatching.
    alt_up(&mut m);
    let repeat = push_output(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    alt_down(&mut m);
    let repeat = push_output(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed && repeat.announce);
    // Mid-hold suppression gates dispatch but keeps the pair.
    m.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: true,
        alt: true,
    }]);
    let repeat = push_output(&mut m, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    let up = push_output(&mut m, VK_H, true, true, false).expect("up");
    assert!(up.consumed);
    // Fresh presses afterwards pass through.
    assert_eq!(SnapClassify::push(&mut m, VK_H, false, true, false), None);
}

#[test]
fn output_queue_carries_origin_and_mask_trigger() {
    // Queue/mask routing: output classifies and queues with announce, and a
    // consumed chord arms the Start-menu mask with the output trigger.
    use tiler_windows::snapkey::{
        MaskTrigger, QueuedWorkspaceOutputIntent, SnapQueue, win_up_mask_reserve,
    };
    let mut m = SnapClassify::new(takeover());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_CONTROL, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_MENU, false, None, false, tick, true),
        None
    );
    let origin = Some(origin_of(7, "w7"));
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_L,
            false,
            origin.clone(),
            false,
            tick,
            true
        ),
        Some(true)
    );
    match q.pop_front().expect("output") {
        QueuedSnapEvent::WorkspaceOutput(queued) => {
            assert_eq!(
                queued,
                QueuedWorkspaceOutputIntent {
                    direction: Direction::Right,
                    follow: true,
                    edge: SnapEdge::Down,
                    origin,
                    consumed: true,
                    announce: true,
                    tick: queued.tick,
                }
            );
        }
        other => panic!("expected output, got {other:?}"),
    }
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(
            mask.trigger,
            MaskTrigger::WorkspaceOutput {
                direction: Direction::Right,
                follow: true,
            }
        ),
        other => panic!("expected mask, got {other:?}"),
    }
}

// Item 14 whole-workspace migration: no direct chord (unbound follow-only by
// default); a rebound Win+Ctrl+Alt+Shift chord routes into the canonical
// letter slot through the migrate action. Held repeats stay swallowed (a
// held migration never re-fires); ups close the pair consumed.
fn push_migrate(
    m: &mut SnapClassify,
    vk: u32,
    is_up: bool,
    fg: bool,
    inj: bool,
) -> Option<tiler_windows::snapkey::WorkspaceMigrateIntent> {
    match SnapClassify::push(m, vk, is_up, fg, inj)? {
        Classified::WorkspaceMigrate(intent) => Some(intent),
        Classified::Snap(_)
        | Classified::Workspace(_)
        | Classified::WorkspaceSend(_)
        | Classified::WorkspaceOutput(_)
        | Classified::WorkspaceHistory(_)
        | Classified::Maximize(_)
        | Classified::Fullscreen(_)
        | Classified::Float(_)
        | Classified::Orientation(_)
        | Classified::Sticky(_)
        | Classified::Launch(_)
        | Classified::WorkspaceTiling(_)
        | Classified::Resize(_) => {
            panic!("expected workspace migration")
        }
    }
}

fn shift_ctrl_alt_down(m: &mut SnapClassify) {
    push_snap(m, VK_SHIFT, false, true, false);
    push_snap(m, VK_CONTROL, false, true, false);
    alt_down(m);
}

#[test]
fn migrate_direct_full_modifier_chord_refuses_without_rebind() {
    // Win+Ctrl+Alt+Shift+H passes through untracked with no pin (migration
    // is rebind-only), so the paired key-up passes through as well.
    let mut m = SnapClassify::new(takeover());
    win_down(&mut m, VK_LWIN);
    shift_ctrl_alt_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_H, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_H, true, true, false), None);
    assert_eq!(m.migrate_counts[0].down, 0);
}

#[test]
fn migrate_rebind_routes_all_four_directions() {
    // Four rebound chords (one per direction, distinct physical keys) route
    // into their canonical letter slots with the exact direction; partial
    // modifiers never route.
    use tiler_windows::snapkey::{ChordAction, ChordRemap};
    const VK_F6: u32 = 0x75;
    const VK_F7: u32 = 0x76;
    const VK_F8: u32 = 0x77;
    const VK_F9: u32 = 0x78;
    let cases: &[(u32, u32, Direction)] = &[
        (VK_F6, VK_H, Direction::Left),
        (VK_F7, VK_L, Direction::Right),
        (VK_F8, VK_K, Direction::Up),
        (VK_F9, VK_J, Direction::Down),
    ];
    for (physical, canon, direction) in cases {
        let mut m = SnapClassify::new(takeover());
        m.set_remap(vec![ChordRemap {
            from_vk: *physical,
            from_shift: true,
            from_ctrl: true,
            from_alt: true,
            action: ChordAction::WorkspaceMigrate,
            to_vk: *canon,
        }]);
        win_down(&mut m, VK_LWIN);
        shift_ctrl_alt_down(&mut m);
        let down = push_migrate(&mut m, *physical, false, true, false).expect("down");
        assert_eq!(down.direction, *direction);
        assert_eq!(down.edge, SnapEdge::Down);
        assert!(down.consumed && down.announce);
        // Held repeats stay swallowed, never re-dispatched.
        let repeat = push_migrate(&mut m, *physical, false, true, false).expect("repeat");
        assert_eq!(repeat.direction, *direction);
        assert!(repeat.consumed && !repeat.announce);
        let up = push_migrate(&mut m, *physical, true, true, false).expect("up");
        assert_eq!((up.direction, up.edge), (*direction, SnapEdge::Up));
        assert!(up.consumed && !up.announce);
        // Fresh presses afterwards route again (pair closed, slot free).
        let down = push_migrate(&mut m, *physical, false, true, false).expect("down");
        assert!(down.consumed && down.announce);
    }
}

#[test]
fn migrate_partial_modifiers_never_route_even_rebound() {
    // A rebound expecting the full Win+Ctrl+Alt+Shift arm never fires on a
    // partial chord: the physical routes as itself and passes untracked.
    use tiler_windows::snapkey::{ChordAction, ChordRemap};
    const VK_F6: u32 = 0x75;
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_F6,
        from_shift: true,
        from_ctrl: true,
        from_alt: true,
        action: ChordAction::WorkspaceMigrate,
        to_vk: VK_H,
    }]);
    win_down(&mut m, VK_LWIN);
    // Missing Shift: F6 is no catalog key, so nothing classifies.
    push_snap(&mut m, VK_CONTROL, false, true, false);
    alt_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_F6, false, true, false), None);
    // Missing Win entirely: passes untracked too.
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_F6,
        from_shift: true,
        from_ctrl: true,
        from_alt: true,
        action: ChordAction::WorkspaceMigrate,
        to_vk: VK_H,
    }]);
    push_snap(&mut m, VK_SHIFT, false, true, false);
    push_snap(&mut m, VK_CONTROL, false, true, false);
    alt_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_F6, false, true, false), None);
}

#[test]
fn migrate_repeat_swallowed_and_suppression_keeps_pair() {
    // Mid-hold suppression gates dispatch (already swallowed) but keeps the
    // consumed pair; fresh presses afterwards pass through.
    use tiler_windows::snapkey::{ChordAction, ChordDisable, ChordRemap};
    const VK_F6: u32 = 0x75;
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_F6,
        from_shift: true,
        from_ctrl: true,
        from_alt: true,
        action: ChordAction::WorkspaceMigrate,
        to_vk: VK_H,
    }]);
    win_down(&mut m, VK_LWIN);
    shift_ctrl_alt_down(&mut m);
    let down = push_migrate(&mut m, VK_F6, false, true, false).expect("down");
    assert!(down.consumed && down.announce);
    m.set_disabled(vec![ChordDisable {
        vk: VK_F6,
        shift: true,
        ctrl: true,
        alt: true,
    }]);
    let repeat = push_migrate(&mut m, VK_F6, false, true, false).expect("repeat");
    assert!(repeat.consumed && !repeat.announce);
    let up = push_migrate(&mut m, VK_F6, true, true, false).expect("up");
    assert!(up.consumed);
    // The rebound table wins over suppression for fresh downs, so the next
    // press still routes; removing the rebind lets the suppression pass the
    // physical through.
    let down = push_migrate(&mut m, VK_F6, false, true, false).expect("down");
    assert!(down.consumed && down.announce);
    let up = push_migrate(&mut m, VK_F6, true, true, false).expect("up");
    assert!(up.consumed);
    m.set_remap(Vec::new());
    assert_eq!(SnapClassify::push(&mut m, VK_F6, false, true, false), None);
}

#[test]
fn migrate_queue_carries_direction_and_mask_trigger() {
    // Queue/mask routing: migration classifies and queues with announce, and
    // a consumed chord arms the Start-menu mask with the migrate trigger.
    use tiler_windows::snapkey::{
        ChordAction, ChordRemap, MaskTrigger, QueuedWorkspaceMigrateIntent, SnapQueue,
        win_up_mask_reserve,
    };
    const VK_F6: u32 = 0x75;
    let mut m = SnapClassify::new(takeover());
    m.set_remap(vec![ChordRemap {
        from_vk: VK_F6,
        from_shift: true,
        from_ctrl: true,
        from_alt: true,
        action: ChordAction::WorkspaceMigrate,
        to_vk: VK_H,
    }]);
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_SHIFT, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_CONTROL, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_MENU, false, None, false, tick, true),
        None
    );
    let origin = Some(origin_of(7, "w7"));
    assert_eq!(
        classify_and_queue(
            &mut m,
            &mut q,
            VK_F6,
            false,
            origin.clone(),
            false,
            tick,
            true
        ),
        Some(true)
    );
    match q.pop_front().expect("migrate") {
        QueuedSnapEvent::WorkspaceMigrate(queued) => {
            assert_eq!(
                queued,
                QueuedWorkspaceMigrateIntent {
                    direction: Direction::Left,
                    edge: SnapEdge::Down,
                    origin,
                    consumed: true,
                    announce: true,
                    tick: queued.tick,
                }
            );
        }
        other => panic!("expected migrate, got {other:?}"),
    }
    assert!(win_up_mask_reserve(&mut m, &mut q, VK_LWIN, true, tick));
    match q.pop_front().expect("mask") {
        QueuedSnapEvent::Mask(mask) => assert_eq!(
            mask.trigger,
            MaskTrigger::WorkspaceMigrate {
                direction: Direction::Left,
            }
        ),
        other => panic!("expected mask, got {other:?}"),
    }
}
