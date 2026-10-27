#![forbid(unsafe_code)]
//! Offline moving-border comparison: Core-live absolute shapes vs a
//! Core-anchored test-only descriptor (eligibility + relative outsets derived
//! from the real core shape, applied mechanically to the live frame).
//! Both candidates share one exact source-causal structural gate (focus,
//! lifetime, fullscreen, layout, gesture, common epoch) plus monotonic
//! latest-publish order; geometry never gates. Underlay paints the real reply
//! shape verbatim only when the exact observation generation is current.

use std::collections::BTreeMap;
use std::time::Instant;

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::gesture::{GestureIngress, GestureKind, GesturePhase};
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::visual::BorderHideReason;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_presentation::PresentationUnderlay;
use tiler_core::world_report::{
    CommandResult, PresentationIngress, ScreenReport, TilingMode, WORLD_CONTRACT_VERSION,
    WindowFact, WorldCommand, WorldReply, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 1600, 600)),
        work_area: Some(rect(x, 0, 1600, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary: x == 0,
    }
}

fn fact(window: &str, x: i32, w: i32, h: i32, fullscreen: bool) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId("out-a".to_owned()),
        rect: Some(rect(x, 0, w, h)),
        floating: false,
        fit_excluded: false,
        fullscreen,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
        native_workspace: None,
    }
}

fn ingress(token: &str, frame: Option<Rect>, fullscreen: bool) -> PresentationIngress {
    PresentationIngress {
        foreground: token.to_owned(),
        foreground_fullscreen: fullscreen,
        foreground_frame: frame,
        border_enabled: true,
        underlay_enabled: true,
        border_gap_px: 2,
        border_width_px: 4,
        underlay_extension_px: 3,
        chord_held: true,
        move_window: token.to_owned(),
        ..PresentationIngress::disabled()
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    req: u64,
    obs: u64,
    windows: Vec<WindowFact>,
    closed: Vec<WindowId>,
    focused: &str,
    frame: Option<Rect>,
    fs: bool,
    cseq: Option<u64>,
    cmd: Option<WorldCommand>,
    gesture: Option<GestureIngress>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: obs,
        request_seq: req,
        request_id: format!("req-{req}"),
        command_seq: cseq,
        command: cmd,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0), screen("out-b", 1600)],
        windows,
        views: Vec::new(),
        closed,
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        default_tiled: true,
        born_maximized: tiler_core::born_maximized::BornMaximized::default(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture,
        monotonic_ms: obs * 16,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: ingress(focused, frame, fs),
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Witness {
    subject: String,
    lifetime: u64,
    fullscreen: bool,
    layout: u64,
    gesture: Option<(String, u64)>,
    epoch: u64,
}

#[derive(Clone, Debug)]
struct Publication {
    pub_seq: u64,
    obs_seq: u64,
    exact_gen: u64,
    subject: String,
    visible: bool,
    reason: Option<BorderHideReason>,
    inner: Option<Rect>,
    outer: Option<Rect>,
    frame_at_reply: Option<Rect>,
    witness: Witness,
    underlay: PresentationUnderlay,
}

#[derive(Clone, Copy, Debug)]
struct AnchoredDescriptor {
    eligible: bool,
    inner_d: (i32, i32, i32, i32),
    outer_d: (i32, i32, i32, i32),
}

fn describe(p: &Publication) -> Option<AnchoredDescriptor> {
    let frame = p.frame_at_reply?;
    let inner = p.inner?;
    let outer = p.outer?;
    if !p.visible {
        return Some(AnchoredDescriptor {
            eligible: false,
            inner_d: (0, 0, 0, 0),
            outer_d: (0, 0, 0, 0),
        });
    }
    Some(AnchoredDescriptor {
        eligible: true,
        inner_d: (
            inner.x - frame.x,
            inner.y - frame.y,
            inner.w - frame.w,
            inner.h - frame.h,
        ),
        outer_d: (
            outer.x - inner.x,
            outer.y - inner.y,
            outer.w - inner.w,
            outer.h - inner.h,
        ),
    })
}

fn apply_descriptor(d: AnchoredDescriptor, live: &Rect) -> Option<Rect> {
    if !d.eligible {
        return None;
    }
    let inner = Rect {
        x: live.x + d.inner_d.0,
        y: live.y + d.inner_d.1,
        w: live.w + d.inner_d.2,
        h: live.h + d.inner_d.3,
    };
    Some(Rect {
        x: inner.x + d.outer_d.0,
        y: inner.y + d.outer_d.1,
        w: inner.w + d.outer_d.2,
        h: inner.h + d.outer_d.3,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Live,
    Anchored,
}

#[derive(Default)]
struct CaseMetrics {
    stale_rejects: u64,
    recoveries: u64,
    cross_paints: u64,
    underlay_refusals: u64,
}

struct Candidate {
    mode: Mode,
    painted_pub: Option<u64>,
    painted_outer: Option<Rect>,
    painted_visible: bool,
    painted_witness: Option<Witness>,
    underlay_painted: Option<Rect>,
    underlay_gen: Option<u64>,
    metrics: CaseMetrics,
}

impl Candidate {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            painted_pub: None,
            painted_outer: None,
            painted_visible: false,
            painted_witness: None,
            underlay_painted: None,
            underlay_gen: None,
            metrics: CaseMetrics::default(),
        }
    }
    fn painted_shape(&self, p: &Publication, live_frame: &Rect) -> Option<Rect> {
        if !p.visible {
            return None;
        }
        match self.mode {
            Mode::Live => p.outer,
            Mode::Anchored => apply_descriptor(describe(p)?, live_frame),
        }
    }
    fn offer(
        &mut self,
        p: &Publication,
        live: &Witness,
        live_frame: &Rect,
        live_gesture: &Option<(String, u64)>,
        latest_pub: u64,
    ) -> bool {
        if self.painted_pub.is_some_and(|q| p.pub_seq <= q) || p.pub_seq < latest_pub {
            self.metrics.stale_rejects += 1;
            return false;
        }
        if p.witness != *live {
            self.metrics.stale_rejects += 1;
            return false;
        }
        if let Some(g) = &p.witness.gesture
            && Some(g) != live_gesture.as_ref()
        {
            self.metrics.cross_paints += 1;
            return false;
        }
        let was = self.painted_visible;
        self.painted_pub = Some(p.pub_seq);
        self.painted_witness = Some(p.witness.clone());
        self.painted_outer = self.painted_shape(p, live_frame);
        self.painted_visible = self.painted_outer.is_some();
        if self.painted_visible && !was {
            self.metrics.recoveries += 1;
        }
        true
    }
    fn retained_border_check(&mut self, live: &Witness) {
        if self.painted_visible && self.painted_witness.as_ref().is_some_and(|w| w != live) {
            self.painted_visible = false;
            self.painted_outer = None;
        }
    }
    fn offer_underlay(&mut self, p: &Publication, live_exact: u64) -> bool {
        if p.exact_gen != live_exact {
            self.metrics.underlay_refusals += 1;
            return false;
        }
        self.underlay_gen = Some(p.exact_gen);
        self.underlay_painted = match &p.underlay {
            PresentationUnderlay::Found(f) => Some(f.outer_rect),
            PresentationUnderlay::Hidden { .. } => None,
        };
        true
    }
    fn retained_underlay_check(&mut self, live_exact: u64) {
        if self.underlay_gen.is_some_and(|g| g != live_exact) {
            self.underlay_gen = None;
            self.underlay_painted = None;
        }
    }
}

fn live_ideal_outer(f: &Rect) -> Rect {
    Rect {
        x: f.x - 6,
        y: f.y - 6,
        w: f.w + 12,
        h: f.h + 12,
    }
}

fn accept_reply(engine: &mut WorldEngine, rep: &WorldReport, what: &str) -> WorldReply {
    match engine.handle_report(rep) {
        WorldOutcome::Accepted(r)
        | WorldOutcome::Duplicate(r)
        | WorldOutcome::CommandConsumed(r) => r,
        WorldOutcome::Rejected { kind, message } => panic!("{what} rejected: {kind} {message}"),
    }
}

fn assert_no_preview(case: &str, reply: &WorldReply) {
    assert!(
        !matches!(
            reply.command_result,
            Some(CommandResult::DragPreview { .. })
        ),
        "{case}: non-gesture report carries no DragPreview command_result"
    );
}

struct LiveSource {
    frames: BTreeMap<String, Rect>,
    foreground: String,
    fullscreen_a: bool,
    layout: u64,
    gesture: Option<(String, u64)>,
    lifetimes: BTreeMap<String, u64>,
    epoch: u64,
    exact_gen: u64,
    obs: u64,
    req: u64,
}

impl LiveSource {
    fn witness(&self) -> Witness {
        Witness {
            subject: self.foreground.clone(),
            lifetime: self.lifetimes.get(&self.foreground).copied().unwrap_or(0),
            fullscreen: self.foreground == "win-a" && self.fullscreen_a,
            layout: self.layout,
            gesture: self.gesture.clone(),
            epoch: self.epoch,
        }
    }
    fn set_foreground(&mut self, next: &str) {
        if self.foreground != next {
            self.foreground = next.to_owned();
            self.epoch += 1;
            self.exact_gen += 1;
        }
    }
    fn set_fullscreen_a(&mut self, next: bool) {
        if self.fullscreen_a != next {
            self.fullscreen_a = next;
            self.epoch += 1;
            self.exact_gen += 1;
        }
    }
    fn bump_layout(&mut self) {
        self.layout += 1;
        self.epoch += 1;
        self.exact_gen += 1;
    }
    fn set_gesture(&mut self, next: Option<(String, u64)>) {
        if self.gesture != next {
            self.gesture = next;
            self.epoch += 1;
            self.exact_gen += 1;
        }
    }
    fn close_window(&mut self, name: &str, fallback: &str) {
        self.frames.remove(name);
        self.foreground = fallback.to_owned();
        self.epoch += 1;
        self.exact_gen += 1;
    }
    fn reopen_window(&mut self, name: &str, frame: Rect, next_lifetime: u64) {
        self.frames.insert(name.to_owned(), frame);
        self.lifetimes.insert(name.to_owned(), next_lifetime);
        self.foreground = name.to_owned();
        self.epoch += 1;
        self.exact_gen += 1;
    }
    fn set_frame_geometry(&mut self, name: &str, frame: Rect) {
        self.frames.insert(name.to_owned(), frame);
        self.exact_gen += 1;
    }
}

fn live_frame(live: &LiveSource, subject: &str) -> Rect {
    live.frames
        .get(subject)
        .copied()
        .unwrap_or(rect(0, 0, 0, 0))
}

fn base_windows(live: &LiveSource) -> Vec<WindowFact> {
    let mut out = Vec::new();
    for (name, f) in &live.frames {
        let fs = name == "win-a" && live.fullscreen_a;
        let mut wf = fact(name, f.x, f.w, f.h, fs);
        if name == "win-c" {
            wf.output = OutputId("out-b".to_owned());
        }
        out.push(wf);
    }
    out
}

fn publish_from_reply(
    pub_seq: u64,
    reply: &WorldReply,
    frame_at_reply: Option<Rect>,
    witness: Witness,
    exact_gen: u64,
    obs_seq: u64,
) -> Publication {
    Publication {
        pub_seq,
        obs_seq,
        exact_gen,
        subject: reply.presentation.foreground.clone(),
        visible: reply.presentation.border.visible,
        reason: reply.presentation.border.reason,
        inner: reply.presentation.border.inner,
        outer: reply.presentation.border.outer,
        frame_at_reply,
        witness,
        underlay: reply.presentation.underlay.clone(),
    }
}

fn gesture_ingress(
    id: u64,
    window: &str,
    phase: GesturePhase,
    cx: i32,
    start: Rect,
) -> GestureIngress {
    GestureIngress {
        id,
        window: WindowId(window.to_owned()),
        kind: GestureKind::Move,
        cursor: Some((cx, 10)),
        start_cursor: Some((100, 10)),
        phase,
        esc: false,
        modal_move: true,
        start_frame: start,
    }
}

fn fresh_live() -> LiveSource {
    LiveSource {
        frames: BTreeMap::from([
            ("win-a".to_owned(), rect(100, 0, 40, 90)),
            ("win-b".to_owned(), rect(400, 0, 40, 90)),
            ("win-c".to_owned(), rect(1600, 0, 40, 90)),
        ]),
        foreground: "win-a".to_owned(),
        fullscreen_a: false,
        layout: 0,
        gesture: None,
        lifetimes: BTreeMap::from([
            ("win-a".to_owned(), 1),
            ("win-b".to_owned(), 1),
            ("win-c".to_owned(), 1),
        ]),
        epoch: 0,
        exact_gen: 0,
        obs: 0,
        req: 0,
    }
}

fn next_report(
    live: &mut LiveSource,
    gesture: Option<GestureIngress>,
    command: Option<(u64, WorldCommand)>,
) -> WorldReport {
    live.obs += 1;
    live.req += 1;
    let frame = live.frames.get(&live.foreground).copied();
    let fs = live.foreground == "win-a" && live.fullscreen_a;
    let (cseq, cmd) = command
        .map(|(s, c)| (Some(s), Some(c)))
        .unwrap_or((None, None));
    report(
        live.req,
        live.obs,
        base_windows(live),
        Vec::new(),
        &live.foreground.clone(),
        frame,
        fs,
        cseq,
        cmd,
        gesture,
    )
}

fn assert_fresh_old_refuses(case: &str, held: &Publication, live: &LiveSource) {
    let wit = live.witness();
    let frame = live_frame(live, &held.subject);
    let mut l = Candidate::new(Mode::Live);
    let mut a = Candidate::new(Mode::Anchored);
    assert!(
        !l.offer(held, &wit, &frame, &live.gesture, held.pub_seq),
        "{case}: fresh live refuses held old"
    );
    assert!(
        !a.offer(held, &wit, &frame, &live.gesture, held.pub_seq),
        "{case}: fresh anchored refuses held old"
    );
    for (name, c) in [("live", &l), ("anchored", &a)] {
        assert_eq!(c.painted_pub, None, "{case}: fresh {name} paints no pub");
        assert_eq!(
            c.painted_outer, None,
            "{case}: fresh {name} paints no shape"
        );
        assert!(!c.painted_visible, "{case}: fresh {name} stays hidden");
    }
}

// Shared fixture: one engine + live source + both candidates + pub counter.
// Witnesses are captured at observation time; exact gens at observation.
struct Fx {
    eng: WorldEngine,
    live: LiveSource,
    l: Candidate,
    a: Candidate,
    pseq: u64,
    bns: u128,
    hns: u128,
    wns: u128,
    ons: u128,
}

impl Fx {
    fn new() -> Self {
        Self {
            eng: WorldEngine::new("gen-1").expect("generation"),
            live: fresh_live(),
            l: Candidate::new(Mode::Live),
            a: Candidate::new(Mode::Anchored),
            pseq: 0,
            bns: 0,
            hns: 0,
            wns: 0,
            ons: 0,
        }
    }
    fn publish(
        &mut self,
        what: &str,
        gesture: Option<GestureIngress>,
        cmd: Option<(u64, WorldCommand)>,
    ) -> (WorldReply, Publication) {
        let t0 = Instant::now();
        let rep = next_report(&mut self.live, gesture.clone(), cmd);
        self.bns += t0.elapsed().as_nanos();
        let t1 = Instant::now();
        let reply = accept_reply(&mut self.eng, &rep, what);
        self.hns += t1.elapsed().as_nanos();
        if gesture.is_none() {
            assert_no_preview(what, &reply);
        }
        let wit = self.live.witness();
        let t2 = Instant::now();
        self.pseq += 1;
        let p = publish_from_reply(
            self.pseq,
            &reply,
            self.live.frames.get(&self.live.foreground).copied(),
            wit,
            self.live.exact_gen,
            self.live.obs,
        );
        self.wns += t2.elapsed().as_nanos();
        (reply, p)
    }
    fn paint_both(&mut self, p: &Publication, latest: u64) -> (bool, bool) {
        let wit = self.live.witness();
        let f = live_frame(&self.live, &p.subject);
        let t = Instant::now();
        let pl = self.l.offer(p, &wit, &f, &self.live.gesture, latest);
        let pa = self.a.offer(p, &wit, &f, &self.live.gesture, latest);
        self.ons += t.elapsed().as_nanos();
        (pl, pa)
    }
    fn check_shapes(&self, case: &str, p: &Publication) {
        assert_eq!(
            self.l.painted_outer, p.outer,
            "{case}: live paints absolute core shape"
        );
        let f = live_frame(&self.live, &p.subject);
        assert_eq!(
            self.a.painted_outer,
            Some(live_ideal_outer(&f)),
            "{case}: anchored tracks live frame"
        );
    }
    fn invalidate(&mut self) {
        let wit = self.live.witness();
        self.l.retained_border_check(&wit);
        self.a.retained_border_check(&wit);
        self.l.retained_underlay_check(self.live.exact_gen);
        self.a.retained_underlay_check(self.live.exact_gen);
    }
    fn log(&self, case: &str) {
        println!(
            "case={case} stale_live={} stale_anchored={} cost_ns_illustrative build={} handle={} publish_wrap={} offer={}",
            self.l.metrics.stale_rejects,
            self.a.metrics.stale_rejects,
            self.bns,
            self.hns,
            self.wns,
            self.ons
        );
    }
}

fn boundary_round(
    case: &str,
    mutate: impl FnOnce(&mut LiveSource),
    expect_visible: bool,
) -> (u64, u64) {
    let mut fx = Fx::new();
    let (_, pub1) = fx.publish("baseline", None, None);
    assert!(pub1.visible, "{case}: baseline visible");
    assert_eq!(pub1.reason, None);
    let wit0 = pub1.witness.clone();
    let (pl, pa) = fx.paint_both(&pub1, pub1.pub_seq);
    assert!(pl && pa, "{case}: baseline paint");
    fx.check_shapes(case, &pub1);
    assert!(fx.l.offer_underlay(&pub1, fx.live.exact_gen));
    assert!(fx.a.offer_underlay(&pub1, fx.live.exact_gen));
    mutate(&mut fx.live);
    let wit1 = fx.live.witness();
    assert_ne!(wit0, wit1, "{case}: boundary advances causal witness");
    assert_fresh_old_refuses(case, &pub1, &fx.live);
    fx.invalidate();
    assert!(
        !fx.l.painted_visible && fx.l.painted_outer.is_none(),
        "{case}: live retained hides"
    );
    assert!(
        !fx.a.painted_visible && fx.a.painted_outer.is_none(),
        "{case}: anchored retained hides"
    );
    let f1 = live_frame(&fx.live, &pub1.subject);
    assert!(
        !fx.l.offer(&pub1, &wit1, &f1, &fx.live.gesture, 1),
        "{case}: live old refuses"
    );
    assert!(
        !fx.a.offer(&pub1, &wit1, &f1, &fx.live.gesture, 1),
        "{case}: anchored old refuses"
    );
    assert!(
        fx.l.underlay_painted.is_none() && fx.a.underlay_painted.is_none(),
        "{case}: retained underlay hides on boundary"
    );
    assert!(
        !fx.l.offer_underlay(&pub1, fx.live.exact_gen),
        "{case}: old underlay refuses (live)"
    );
    assert!(
        !fx.a.offer_underlay(&pub1, fx.live.exact_gen),
        "{case}: old underlay refuses (anchored)"
    );
    let (_, cov) = fx.publish("covering", None, None);
    assert_eq!(
        fx.live.witness(),
        wit1,
        "{case}: covering carries boundary state"
    );
    let (ql, qa) = fx.paint_both(&cov, cov.pub_seq);
    if expect_visible {
        assert!(ql && qa, "{case}: covering recovers");
        assert!(fx.l.painted_visible && fx.a.painted_visible);
        fx.check_shapes(case, &cov);
    } else {
        assert!(!cov.visible, "{case}: fullscreen reply stays hidden");
        assert!(fx.l.painted_outer.is_none() && fx.a.painted_outer.is_none());
    }
    match &cov.underlay {
        PresentationUnderlay::Found(f) => {
            assert!(fx.l.offer_underlay(&cov, fx.live.exact_gen));
            assert!(fx.a.offer_underlay(&cov, fx.live.exact_gen));
            assert_eq!(fx.l.underlay_painted, Some(f.outer_rect));
            assert_eq!(fx.a.underlay_painted, Some(f.outer_rect));
        }
        PresentationUnderlay::Hidden { .. } => {}
    }
    fx.log(case);
    (fx.l.metrics.stale_rejects, fx.a.metrics.stale_rejects)
}

fn workspace_of(reply: &WorldReply) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("logical view for out-a")
}

#[test]
fn focus_away_then_return() {
    let away = boundary_round("focus-away", |live| live.set_foreground("win-b"), true);
    assert!(away.0 > 0 && away.1 > 0);
    let mut fx = Fx::new();
    fx.live.set_foreground("win-b");
    let (_, pub1) = fx.publish("return-baseline", None, None);
    assert!(pub1.visible);
    let (pl, pa) = fx.paint_both(&pub1, pub1.pub_seq);
    assert!(pl && pa);
    fx.live.set_foreground("win-a");
    let wit1 = fx.live.witness();
    assert_fresh_old_refuses("focus-return-fresh", &pub1, &fx.live);
    fx.invalidate();
    assert!(!fx.l.painted_visible && !fx.a.painted_visible);
    let f = live_frame(&fx.live, &pub1.subject);
    assert!(!fx.l.offer(&pub1, &wit1, &f, &fx.live.gesture, 1));
    assert!(!fx.a.offer(&pub1, &wit1, &f, &fx.live.gesture, 1));
    let (_, cov) = fx.publish("return-covering", None, None);
    let (ql, qa) = fx.paint_both(&cov, cov.pub_seq);
    assert!(ql && qa && fx.l.painted_visible && fx.a.painted_visible);
    fx.check_shapes("focus-return", &cov);
    fx.log("focus-return");
    assert!(fx.l.metrics.stale_rejects > 0 && fx.a.metrics.stale_rejects > 0);
    let mut fx2 = Fx::new();
    let (_, pub_a) = fx2.publish("aba-a", None, None);
    let wit_a = pub_a.witness.clone();
    fx2.live.set_foreground("win-b");
    let wit_b = fx2.live.witness();
    assert_ne!(wit_a, wit_b);
    assert_fresh_old_refuses("focus-aba-away", &pub_a, &fx2.live);
    let (_, pub_b) = fx2.publish("aba-b", None, None);
    fx2.live.set_foreground("win-a");
    let wit_ret = fx2.live.witness();
    assert_ne!(
        wit_a, wit_ret,
        "ABA return epoch disambiguates subject equality"
    );
    assert_ne!(wit_b, wit_ret);
    assert_fresh_old_refuses("focus-aba-ret-a", &pub_a, &fx2.live);
    assert_fresh_old_refuses("focus-aba-ret-b", &pub_b, &fx2.live);
    let (_, pub_c) = fx2.publish("aba-a2", None, None);
    let (rl, ra) = fx2.paint_both(&pub_c, pub_c.pub_seq);
    assert!(rl && ra && fx2.l.painted_visible && fx2.a.painted_visible);
    println!(
        "case=focus-aba stale_live={} stale_anchored={}",
        fx2.l.metrics.stale_rejects, fx2.a.metrics.stale_rejects
    );
}

#[test]
fn close_same_handle_fresh_lifetime() {
    let mut fx = Fx::new();
    let (_, pub1) = fx.publish("close-baseline", None, None);
    assert!(pub1.visible);
    let (pl, pa) = fx.paint_both(&pub1, pub1.pub_seq);
    assert!(pl && pa);
    let f0 = live_frame(&fx.live, &pub1.subject);
    let wit0 = pub1.witness.clone();
    fx.live.obs += 1;
    fx.live.req += 1;
    fx.live.close_window("win-a", "win-b");
    let close_rep = report(
        fx.live.req,
        fx.live.obs,
        base_windows(&fx.live),
        vec![WindowId("win-a".to_owned())],
        &fx.live.foreground.clone(),
        fx.live.frames.get(&fx.live.foreground).copied(),
        false,
        None,
        None,
        None,
    );
    let _ = accept_reply(&mut fx.eng, &close_rep, "close");
    let wit1 = fx.live.witness();
    assert_ne!(wit0, wit1);
    assert_ne!(wit0.subject, wit1.subject);
    assert_fresh_old_refuses("close-fresh", &pub1, &fx.live);
    fx.invalidate();
    assert!(
        !fx.l.painted_visible && !fx.a.painted_visible,
        "close hides retained"
    );
    let f1 = live_frame(&fx.live, &pub1.subject);
    assert!(!fx.l.offer(&pub1, &wit1, &f1, &fx.live.gesture, 1));
    assert!(!fx.a.offer(&pub1, &wit1, &f1, &fx.live.gesture, 1));
    fx.live.reopen_window("win-a", rect(140, 0, 52, 96), 2);
    assert_fresh_old_refuses("close-reuse-fresh", &pub1, &fx.live);
    let (_, pub2) = fx.publish("reopen", None, None);
    assert_ne!(
        wit0.lifetime, pub2.witness.lifetime,
        "reopen is a fresh lifetime"
    );
    assert_ne!(wit0, pub2.witness);
    let f2 = live_frame(&fx.live, &pub2.subject);
    let wit2 = fx.live.witness();
    assert!(fx.l.offer(&pub2, &wit2, &f2, &fx.live.gesture, pub2.pub_seq));
    assert!(fx.a.offer(&pub2, &wit2, &f2, &fx.live.gesture, pub2.pub_seq));
    assert!(
        fx.l.painted_visible && fx.a.painted_visible,
        "fresh lifetime recovers"
    );
    assert_eq!(fx.l.painted_outer, pub2.outer);
    assert_eq!(fx.a.painted_outer, Some(live_ideal_outer(&f2)));
    assert_ne!(f2.w, f0.w, "reopen frame differs in size");
    assert!(
        !fx.l
            .offer(&pub1, &wit2, &f0, &fx.live.gesture, pub2.pub_seq)
    );
    assert!(
        !fx.a
            .offer(&pub1, &wit2, &f0, &fx.live.gesture, pub2.pub_seq)
    );
    fx.log("close-reuse");
    assert!(fx.l.metrics.stale_rejects > 0 && fx.a.metrics.stale_rejects > 0);
}

#[test]
fn fullscreen_entry_and_exit() {
    let entry = boundary_round(
        "fullscreen-entry",
        |live| live.set_fullscreen_a(true),
        false,
    );
    assert!(entry.0 > 0 && entry.1 > 0);
    let mut fx = Fx::new();
    fx.live.set_fullscreen_a(true);
    let (_, hid) = fx.publish("fs-baseline", None, None);
    assert!(!hid.visible, "fullscreen baseline hidden");
    assert_eq!(hid.reason, Some(BorderHideReason::Fullscreen));
    fx.live.set_fullscreen_a(false);
    assert_fresh_old_refuses("fullscreen-exit-fresh", &hid, &fx.live);
    let (_, vis) = fx.publish("fs-exit", None, None);
    assert!(vis.visible, "exit recovers visible reply");
    let wit = fx.live.witness();
    let f = live_frame(&fx.live, &vis.subject);
    assert!(fx.l.offer(&vis, &wit, &f, &fx.live.gesture, vis.pub_seq));
    assert!(fx.a.offer(&vis, &wit, &f, &fx.live.gesture, vis.pub_seq));
    assert!(fx.l.painted_visible && fx.a.painted_visible);
    fx.log("fullscreen-exit");
    let mut fx2 = Fx::new();
    let (_, v0) = fx2.publish("fs-aba-visible", None, None);
    assert!(v0.visible);
    let w0 = v0.witness.clone();
    fx2.live.set_fullscreen_a(true);
    assert_ne!(w0, fx2.live.witness());
    assert_fresh_old_refuses("fullscreen-aba-entry", &v0, &fx2.live);
    let (_, hfs) = fx2.publish("fs-aba-hidden", None, None);
    assert!(!hfs.visible);
    fx2.live.set_fullscreen_a(false);
    let w1 = fx2.live.witness();
    assert_ne!(w0, w1, "ABA exit epoch differs");
    assert_fresh_old_refuses("fullscreen-aba-exit-v", &v0, &fx2.live);
    assert_fresh_old_refuses("fullscreen-aba-exit-h", &hfs, &fx2.live);
    let (_, v1) = fx2.publish("fs-aba-visible2", None, None);
    assert!(v1.visible);
    let (rl, ra) = fx2.paint_both(&v1, v1.pub_seq);
    assert!(rl && ra && fx2.l.painted_visible && fx2.a.painted_visible);
    println!(
        "case=fullscreen-aba recovered visible={}",
        fx2.l.painted_visible
    );
}

#[test]
fn layout_change_and_restore() {
    let mut fx = Fx::new();
    let (reply0, pub1) = fx.publish("layout-baseline", None, None);
    assert!(pub1.visible);
    let ws = workspace_of(&reply0);
    assert_eq!(fx.eng.logical_tiling("out-a", &ws), TilingMode::Tiled);
    let (pl, pa) = fx.paint_both(&pub1, pub1.pub_seq);
    assert!(pl && pa);
    let mut seq = 1u64;
    for (case, pub_no, expect) in [
        ("layout-change", 2u64, TilingMode::Floating),
        ("layout-restore", 3u64, TilingMode::Tiled),
    ] {
        fx.live.bump_layout();
        let wit = fx.live.witness();
        assert_fresh_old_refuses(case, &pub1, &fx.live);
        fx.invalidate();
        assert!(
            !fx.l.painted_visible && !fx.a.painted_visible,
            "{case}: retained hides"
        );
        let f_old = live_frame(&fx.live, &pub1.subject);
        assert!(
            !fx.l
                .offer(&pub1, &wit, &f_old, &fx.live.gesture, pub_no - 1)
        );
        assert!(
            !fx.a
                .offer(&pub1, &wit, &f_old, &fx.live.gesture, pub_no - 1)
        );
        seq += 1;
        let cmd = WorldCommand::ToggleWorkspaceTiling {
            output: OutputId("out-a".to_owned()),
            workspace: ws.clone(),
        };
        let (reply, cov) = fx.publish(case, None, Some((seq, cmd)));
        assert_eq!(
            fx.eng.logical_tiling("out-a", &ws),
            expect,
            "{case}: real tiling flips"
        );
        assert_eq!(cov.witness, wit, "{case}: covering carries boundary state");
        let (ql, qa) = fx.paint_both(&cov, pub_no);
        assert!(
            ql && qa && fx.l.painted_visible && fx.a.painted_visible,
            "{case}: recovers"
        );
        fx.check_shapes(case, &cov);
        let _ = reply;
        fx.log(case);
    }
    assert!(fx.l.metrics.stale_rejects >= 2 && fx.a.metrics.stale_rejects >= 2);
}

#[test]
fn gesture_handoff_a_to_b() {
    let mut fx = Fx::new();
    fx.live.set_gesture(Some(("win-a".to_owned(), 7)));
    let start = fx.live.frames["win-a"];
    let (_, pub_a) = fx.publish(
        "gesture-a-start",
        Some(gesture_ingress(
            7,
            "win-a",
            GesturePhase::Started,
            100,
            start,
        )),
        None,
    );
    let wit_a = pub_a.witness.clone();
    let (pl, pa) = fx.paint_both(&pub_a, pub_a.pub_seq);
    assert!(pl && pa && fx.l.painted_visible && fx.a.painted_visible);
    let _ = fx.publish(
        "gesture-a-released",
        Some(gesture_ingress(
            7,
            "win-a",
            GesturePhase::Released,
            140,
            start,
        )),
        None,
    );
    fx.live.set_gesture(None);
    let wit_gap = fx.live.witness();
    assert_ne!(wit_a.gesture, wit_gap.gesture);
    assert_ne!(wit_a, wit_gap);
    fx.invalidate();
    assert!(
        !fx.l.painted_visible && !fx.a.painted_visible,
        "released A hides retained"
    );
    fx.live.set_gesture(Some(("win-a".to_owned(), 8)));
    let wit_b = fx.live.witness();
    assert_ne!(wit_a, wit_b);
    assert_fresh_old_refuses("gesture-handoff-fresh", &pub_a, &fx.live);
    assert!(!fx.l.offer(&pub_a, &wit_b, &start, &fx.live.gesture, 1));
    assert!(!fx.a.offer(&pub_a, &wit_b, &start, &fx.live.gesture, 1));
    assert!(fx.l.painted_outer.is_none() && fx.a.painted_outer.is_none());
    let (_, pub_b) = fx.publish(
        "gesture-b-start",
        Some(gesture_ingress(
            8,
            "win-a",
            GesturePhase::Started,
            140,
            start,
        )),
        None,
    );
    let (ql, qa) = fx.paint_both(&pub_b, pub_b.pub_seq);
    assert!(
        ql && qa && fx.l.painted_visible && fx.a.painted_visible,
        "B recovers same subject"
    );
    assert_eq!(fx.l.metrics.cross_paints, 0, "A never paints while B live");
    assert_eq!(fx.a.metrics.cross_paints, 0);
    assert!(
        !fx.l
            .offer(&pub_a, &wit_b, &start, &fx.live.gesture, pub_b.pub_seq)
    );
    assert!(
        !fx.a
            .offer(&pub_a, &wit_b, &start, &fx.live.gesture, pub_b.pub_seq)
    );
    fx.log("gesture-handoff");
    assert!(fx.l.metrics.stale_rejects > 0 && fx.a.metrics.stale_rejects > 0);
}

#[test]
fn older_publication_after_newer_paint_refuses() {
    let mut fx = Fx::new();
    let (_, pub1) = fx.publish("order-first", None, None);
    let wit = pub1.witness.clone();
    let reply2 = accept_reply(
        &mut fx.eng,
        &next_report(&mut fx.live, None, None),
        "order-second",
    );
    assert_no_preview("order-second", &reply2);
    assert!(pub1.visible && reply2.presentation.border.visible);
    let wit2 = fx.live.witness();
    assert_eq!(wit, wit2, "order probe holds witness constant");
    fx.pseq += 1;
    let pub2 = publish_from_reply(
        fx.pseq,
        &reply2,
        fx.live.frames.get(&fx.live.foreground).copied(),
        wit2.clone(),
        fx.live.exact_gen,
        fx.live.obs,
    );
    for (name, mode) in [("live", Mode::Live), ("anchored", Mode::Anchored)] {
        let mut c = Candidate::new(mode);
        let f = live_frame(&fx.live, &pub2.subject);
        assert!(
            c.offer(&pub2, &wit2, &f, &fx.live.gesture, 2),
            "{name}: newer paints first"
        );
        let (bp, bo, bv) = (c.painted_pub, c.painted_outer, c.painted_visible);
        assert!(
            !c.offer(&pub1, &wit2, &f, &fx.live.gesture, 2),
            "{name}: older after newer refuses by order"
        );
        assert_eq!(c.painted_pub, bp, "{name}: pub never moves backwards");
        assert_eq!(c.painted_outer, bo, "{name}: shape unchanged");
        assert_eq!(c.painted_visible, bv);
        assert!(c.metrics.stale_rejects > 0, "{name}: order refusal counted");
    }
    println!("case=stale-order live_newer_first_ok anchored_newer_first_ok");
}

#[test]
fn moving_tracking_lag_with_virtual_schedule() {
    const TICKS: u64 = 40;
    const CORE_EVERY: u64 = 4;
    const COMPUTE_TICKS: u64 = 3;
    const PUBLISH_DELAY: u64 = 1;
    const PAINT_DELAY: u64 = 1;
    let mut eng = WorldEngine::new("gen-1").expect("generation");
    let mut live = fresh_live();
    let start_frame = live.frames["win-a"];
    live.set_gesture(Some(("win-a".to_owned(), 7)));
    let rep_start = next_report(
        &mut live,
        Some(gesture_ingress(
            7,
            "win-a",
            GesturePhase::Started,
            100,
            start_frame,
        )),
        None,
    );
    let _ = accept_reply(&mut eng, &rep_start, "motion-start");
    let wit_motion = live.witness();
    let mut l = Candidate::new(Mode::Live);
    let mut a = Candidate::new(Mode::Anchored);
    let mut pub_seq = 0u64;
    let mut pending: Option<(WorldReport, u64, Rect, u64)> = None;
    let mut in_flight: Option<(WorldReport, u64, u64, Rect, u64)> = None;
    let mut published: Vec<(Publication, u64, u64, u64, u64)> = Vec::new();
    let mut superseded = 0u64;
    let mut paint_tick_of: BTreeMap<u64, u64> = BTreeMap::new();
    let (mut live_pos_sum, mut live_pos_max, mut live_dim_sum, mut live_dim_max) =
        (0i64, 0i64, 0i64, 0i64);
    let (mut anchored_pos_max, mut anchored_dim_max) = (0i64, 0i64);
    let mut err_samples = 0u64;
    let (mut visible_ticks, mut hidden_ticks) = (0u64, 0u64);
    let (mut age_min, mut age_max, mut age_sum, mut age_samples) = (u64::MAX, 0u64, 0u64, 0u64);
    let (mut ret_min, mut ret_max, mut ret_sum, mut ret_samples) = (u64::MAX, 0u64, 0u64, 0u64);
    let mut obs_start_lags: Vec<u64> = Vec::new();
    let (mut build_ns, mut handle_ns, mut handle_calls, mut wrap_ns, mut offer_ns, mut recomp_ns) =
        (0u128, 0u128, 0u64, 0u128, 0u128, 0u128);
    for tick in 0..TICKS {
        let ax = 100 + 20 * tick as i32;
        let aw = 40 + (tick % 5) as i32 * 4;
        let ah = 90 + (tick % 3) as i32 * 6;
        live.set_frame_geometry("win-a", rect(ax, 0, aw, ah));
        let frame_now = live.frames["win-a"];
        let t0 = Instant::now();
        live.obs += 1;
        live.req += 1;
        let captured_exact = live.exact_gen;
        let rep = report(
            live.req,
            live.obs,
            base_windows(&live),
            Vec::new(),
            "win-a",
            Some(frame_now),
            false,
            None,
            None,
            Some(gesture_ingress(
                7,
                "win-a",
                GesturePhase::Updated,
                ax,
                start_frame,
            )),
        );
        build_ns += t0.elapsed().as_nanos();
        l.retained_underlay_check(live.exact_gen);
        a.retained_underlay_check(live.exact_gen);
        if pending.is_some() {
            superseded += 1;
        }
        pending = Some((rep, tick, frame_now, captured_exact));
        if tick % CORE_EVERY == 0
            && in_flight.is_none()
            && let Some((prep, pobs, pframe, pexact)) = pending.take()
        {
            in_flight = Some((prep, pobs, tick, pframe, pexact));
        }
        if let Some((crep, cobs, cstart, cframe, cexact)) = in_flight.take() {
            if tick >= cstart + COMPUTE_TICKS {
                let t1 = Instant::now();
                let reply = accept_reply(&mut eng, &crep, "motion-complete");
                handle_ns += t1.elapsed().as_nanos();
                handle_calls += 1;
                assert!(
                    cexact < live.exact_gen,
                    "captured exact gen stays older than live exact at delayed completion"
                );
                let t2 = Instant::now();
                pub_seq += 1;
                let pub_ = publish_from_reply(
                    pub_seq,
                    &reply,
                    Some(cframe),
                    wit_motion.clone(),
                    cexact,
                    crep.observation_seq,
                );
                wrap_ns += t2.elapsed().as_nanos();
                assert!(pub_.visible, "steady gesture motion stays eligible");
                assert!(
                    !l.offer_underlay(&pub_, live.exact_gen),
                    "moving underlay refuses (live)"
                );
                assert!(
                    !a.offer_underlay(&pub_, live.exact_gen),
                    "moving underlay refuses (anchored)"
                );
                assert!(l.underlay_painted.is_none() && a.underlay_painted.is_none());
                published.push((pub_, cobs, cstart, tick, tick + PUBLISH_DELAY));
            } else {
                in_flight = Some((crep, cobs, cstart, cframe, cexact));
            }
        }
        let paintable: Vec<u64> = published
            .iter()
            .filter(|(_, _, _, _, tp)| tick >= *tp + PAINT_DELAY)
            .map(|(p, _, _, _, _)| p.pub_seq)
            .collect();
        let latest = paintable.iter().copied().max().unwrap_or(0);
        for (pub_, t_obs, t_start, _, _) in published
            .iter()
            .filter(|(p, _, _, _, _)| paintable.contains(&p.pub_seq))
        {
            if pub_.pub_seq < latest {
                l.metrics.stale_rejects += 1;
                a.metrics.stale_rejects += 1;
                continue;
            }
            let t3 = Instant::now();
            let pl = l.offer(pub_, &wit_motion, &frame_now, &live.gesture, latest);
            let pa = a.offer(pub_, &wit_motion, &frame_now, &live.gesture, latest);
            offer_ns += t3.elapsed().as_nanos();
            if (pl || pa) && !paint_tick_of.contains_key(&pub_.pub_seq) {
                paint_tick_of.insert(pub_.pub_seq, tick);
                let age = tick - t_obs;
                age_min = age_min.min(age);
                age_max = age_max.max(age);
                age_sum += age;
                age_samples += 1;
                obs_start_lags.push(t_start - t_obs);
            }
        }
        l.retained_border_check(&wit_motion);
        a.retained_border_check(&wit_motion);
        if l.painted_visible && a.painted_visible {
            visible_ticks += 1;
            let ideal = live_ideal_outer(&frame_now);
            let t4 = Instant::now();
            let anchored_now = published
                .iter()
                .find(|(p, _, _, _, _)| Some(p.pub_seq) == a.painted_pub)
                .and_then(|(p, _, _, _, _)| describe(p))
                .and_then(|d| apply_descriptor(d, &frame_now))
                .expect("anchored recomputes every repaint");
            recomp_ns += t4.elapsed().as_nanos();
            a.painted_outer = Some(anchored_now);
            let live_now = l.painted_outer.expect("live holds absolute shape");
            let lpos = (live_now.x - ideal.x).abs() as i64 + (live_now.y - ideal.y).abs() as i64;
            let ldim = (live_now.w - ideal.w).abs() as i64 + (live_now.h - ideal.h).abs() as i64;
            let apos =
                (anchored_now.x - ideal.x).abs() as i64 + (anchored_now.y - ideal.y).abs() as i64;
            let adim =
                (anchored_now.w - ideal.w).abs() as i64 + (anchored_now.h - ideal.h).abs() as i64;
            live_pos_sum += lpos;
            live_pos_max = live_pos_max.max(lpos);
            live_dim_sum += ldim;
            live_dim_max = live_dim_max.max(ldim);
            anchored_pos_max = anchored_pos_max.max(apos);
            anchored_dim_max = anchored_dim_max.max(adim);
            err_samples += 1;
            assert_eq!(apos + adim, 0, "anchored recomputes exactly every repaint");
            assert_eq!(l.painted_pub, a.painted_pub, "both track same publication");
            if let Some(seq) = l.painted_pub
                && let Some((_, t_obs, _, _, _)) =
                    published.iter().find(|(p, _, _, _, _)| p.pub_seq == seq)
            {
                let rage = tick - *t_obs;
                ret_min = ret_min.min(rage);
                ret_max = ret_max.max(rage);
                ret_sum += rage;
                ret_samples += 1;
            }
        } else {
            hidden_ticks += 1;
        }
    }
    assert_eq!(
        live.witness(),
        wit_motion,
        "geometry-only motion never advances structural witness"
    );
    let rep_end = next_report(
        &mut live,
        Some(gesture_ingress(
            7,
            "win-a",
            GesturePhase::Released,
            100 + 20 * TICKS as i32,
            start_frame,
        )),
        None,
    );
    let end_reply = accept_reply(&mut eng, &rep_end, "motion-released");
    assert!(
        end_reply.gesture_settled.is_some(),
        "release terminal settles through real core"
    );
    assert!(superseded > 0, "pending slot must supersede observations");
    assert_eq!(
        paint_tick_of.len() + 1,
        published.len(),
        "only pipeline-tail publication unpainted"
    );
    assert!(
        !paint_tick_of.contains_key(&pub_seq),
        "unpainted must be newest"
    );
    assert!(handle_calls > 1 && err_samples > 0 && visible_ticks > 0 && hidden_ticks > 0);
    assert!(hidden_ticks < visible_ticks);
    assert_eq!(visible_ticks + hidden_ticks, TICKS);
    assert!(live_pos_max > 0, "absolute replay must lag");
    assert!(live_dim_max > 0, "absolute replay must miss size changes");
    assert_eq!(
        anchored_pos_max + anchored_dim_max,
        0,
        "anchored exact every repaint"
    );
    assert!(age_samples > 0 && age_max >= CORE_EVERY && age_min >= 1);
    assert!(ret_samples == visible_ticks || ret_samples == err_samples);
    let mut s_os = (u64::MAX, 0u64, 0u64, 0u64);
    let mut s_sc = (u64::MAX, 0u64, 0u64, 0u64);
    let mut s_cp = (u64::MAX, 0u64, 0u64, 0u64);
    let mut s_pp = (u64::MAX, 0u64, 0u64, 0u64);
    let mut s_tot = (u64::MAX, 0u64, 0u64, 0u64);
    let mut rows = 0u64;
    for (pub_, t_obs, t_start, t_complete, t_publish) in published.iter() {
        if let Some(t_paint) = paint_tick_of.get(&pub_.pub_seq).copied() {
            for (v, s) in [
                (t_start - t_obs, &mut s_os),
                (t_complete - t_start, &mut s_sc),
                (t_publish - t_complete, &mut s_cp),
                (t_paint - t_publish, &mut s_pp),
                (t_paint - t_obs, &mut s_tot),
            ] {
                s.0 = s.0.min(v);
                s.1 = s.1.max(v);
                s.2 += v;
                s.3 += 1;
            }
            rows += 1;
        }
    }
    let mean = |s: (u64, u64, u64, u64)| s.2 as f64 / s.3 as f64;
    println!("motion: ticks={TICKS} completions={handle_calls} superseded={superseded}");
    println!(
        "motion: visible_ticks={visible_ticks} hidden_ticks={hidden_ticks} err_samples={err_samples}"
    );
    println!(
        "motion: live_pos_max={live_pos_max} live_pos_mean={:.2} live_dim_max={live_dim_max} live_dim_mean={:.2}",
        live_pos_sum as f64 / err_samples as f64,
        live_dim_sum as f64 / err_samples as f64
    );
    println!("motion: anchored_pos_max={anchored_pos_max} anchored_dim_max={anchored_dim_max}");
    println!(
        "motion: new_paint_age_min={age_min} max={age_max} mean={:.2}",
        age_sum as f64 / age_samples as f64
    );
    println!(
        "motion: retained_visible_age_min={ret_min} max={ret_max} mean={:.2} samples={ret_samples}",
        ret_sum as f64 / ret_samples as f64
    );
    println!(
        "motion: measured_obs_to_start_min={} max={} mean={:.2}",
        s_os.0,
        s_os.1,
        mean(s_os)
    );
    println!(
        "motion: measured_start_to_complete_min={} max={} mean={:.2}",
        s_sc.0,
        s_sc.1,
        mean(s_sc)
    );
    println!(
        "motion: measured_complete_to_publish_min={} max={} mean={:.2}",
        s_cp.0,
        s_cp.1,
        mean(s_cp)
    );
    println!(
        "motion: measured_publish_to_paint_min={} max={} mean={:.2}",
        s_pp.0,
        s_pp.1,
        mean(s_pp)
    );
    println!(
        "motion: measured_new_reply_total_min={} max={} mean={:.2} rows={rows}",
        s_tot.0,
        s_tot.1,
        mean(s_tot)
    );
    println!(
        "microbench_illustrative_no_thresholds_ns: build={build_ns} handle={handle_ns} calls={handle_calls} publish_wrap={wrap_ns} offer={offer_ns} retained_recompute={recomp_ns}"
    );
    println!("pub_seq,obs_seq,t_obs,t_start,t_complete,t_publish,t_paint,age");
    let mut sorted: Vec<(&Publication, u64, u64, u64, u64)> = published
        .iter()
        .map(|(p, a, b, c, d)| (p, *a, *b, *c, *d))
        .collect();
    sorted.sort_by_key(|(p, _, _, _, _)| p.pub_seq);
    for (pub_, t_obs, t_start, t_complete, t_publish) in sorted {
        match paint_tick_of.get(&pub_.pub_seq).copied() {
            Some(tp) => println!(
                "{},{},{t_obs},{t_start},{t_complete},{t_publish},{tp},{}",
                pub_.pub_seq,
                pub_.obs_seq,
                tp - t_obs
            ),
            None => println!(
                "{},{},{t_obs},{t_start},{t_complete},{t_publish},pending,pending",
                pub_.pub_seq, pub_.obs_seq
            ),
        }
    }
}

#[test]
fn underlay_exact_coverage_group_defined() {
    let mut fx = Fx::new();
    let (_, _) = fx.publish("underlay-admit-1", None, None);
    let (_, pub1) = fx.publish("underlay-admit-2", None, None);
    let wit = fx.live.witness();
    assert_eq!(pub1.witness, wit);
    let found_outer = match &pub1.underlay {
        PresentationUnderlay::Found(f) => {
            assert_eq!(f.members.len(), 2, "expected real admitted A/B pair");
            assert_eq!(f.focused_window, WindowId("win-a".to_owned()));
            let mover = fx.live.frames["win-a"];
            let mover_expanded = Rect {
                x: mover.x - 9,
                y: mover.y - 9,
                w: mover.w + 18,
                h: mover.h + 18,
            };
            assert_ne!(
                f.outer_rect, mover_expanded,
                "underlay group-defined, not mover frame"
            );
            assert!(f.union_rect.w > mover.w, "union spans the pair");
            assert_eq!(
                f.outer_rect,
                Rect {
                    x: f.union_rect.x - 9,
                    y: f.union_rect.y - 9,
                    w: f.union_rect.w + 18,
                    h: f.union_rect.h + 18
                },
                "outer expands real union by gap+width+extension"
            );
            f.outer_rect
        }
        hidden => panic!("stationary covering must carry visible underlay, got {hidden:?}"),
    };
    assert!(
        fx.l.offer_underlay(&pub1, fx.live.exact_gen),
        "stationary paints (live)"
    );
    assert!(
        fx.a.offer_underlay(&pub1, fx.live.exact_gen),
        "stationary paints (anchored)"
    );
    assert_eq!(fx.l.underlay_painted, Some(found_outer));
    assert_eq!(fx.a.underlay_painted, Some(found_outer));
    let wit_before = fx.live.witness();
    fx.live.set_frame_geometry("win-a", rect(180, 0, 56, 102));
    fx.live.obs += 1;
    assert_eq!(
        fx.live.witness(),
        wit_before,
        "geometry-only motion is not structural"
    );
    fx.l.retained_underlay_check(fx.live.exact_gen);
    fx.a.retained_underlay_check(fx.live.exact_gen);
    assert!(
        fx.l.underlay_painted.is_none() && fx.a.underlay_painted.is_none(),
        "motion hides underlay"
    );
    assert!(
        !fx.l.offer_underlay(&pub1, fx.live.exact_gen),
        "stale underlay refuses (live)"
    );
    assert!(
        !fx.a.offer_underlay(&pub1, fx.live.exact_gen),
        "stale underlay refuses (anchored)"
    );
    assert!(fx.l.underlay_painted.is_none() && fx.a.underlay_painted.is_none());
    assert!(fx.l.metrics.underlay_refusals > 0 && fx.a.metrics.underlay_refusals > 0);
    fx.live.req += 1;
    let frame_now = fx.live.frames["win-a"];
    let rep3 = report(
        fx.live.req,
        fx.live.obs,
        base_windows(&fx.live),
        Vec::new(),
        "win-a",
        Some(frame_now),
        false,
        None,
        None,
        None,
    );
    let reply3 = accept_reply(&mut fx.eng, &rep3, "underlay-covering");
    fx.pseq += 1;
    let pub2 = publish_from_reply(
        fx.pseq,
        &reply3,
        Some(frame_now),
        fx.live.witness(),
        fx.live.exact_gen,
        fx.live.obs,
    );
    let found_outer2 = match &pub2.underlay {
        PresentationUnderlay::Found(f) => {
            assert_eq!(f.members.len(), 2, "covering keeps real pair");
            f.outer_rect
        }
        hidden => panic!("covering must restore visible underlay, got {hidden:?}"),
    };
    assert!(fx.l.offer_underlay(&pub2, fx.live.exact_gen));
    assert!(fx.a.offer_underlay(&pub2, fx.live.exact_gen));
    assert_eq!(fx.l.underlay_painted, Some(found_outer2));
    assert_eq!(fx.a.underlay_painted, Some(found_outer2));
    let mover_now = Rect {
        x: frame_now.x - 9,
        y: frame_now.y - 9,
        w: frame_now.w + 18,
        h: frame_now.h + 18,
    };
    assert_ne!(
        found_outer2, mover_now,
        "covering still group-defined, not mover-anchored"
    );
    println!("underlay: stationary_outer={found_outer:?} covering_outer={found_outer2:?}");
    println!(
        "underlay: refusals_live={} refusals_anchored={}",
        fx.l.metrics.underlay_refusals, fx.a.metrics.underlay_refusals
    );
}
