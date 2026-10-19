#![forbid(unsafe_code)]
use crate::input_handoff::{INBOX_CAP, Inbox, InboxItem};
use crate::input_pump::InboxSender;
use crate::render_handoff::{
    BorderDesired, DesiredFrame, RenderCarrier, RenderChannels, WindowLifetime,
};
use crate::render_pump::{FakePresent, RenderCore};
use std::sync::Arc;
use tiler_core::geometry::Rect;

fn test_lifetime() -> WindowLifetime {
    WindowLifetime {
        pid: 7001,
        creation: "creation-7001".to_owned(),
        member_tag: Some("tag-7001".to_owned()),
    }
}

fn test_border(generation: u64, safety: u64) -> DesiredFrame {
    DesiredFrame::Border(BorderDesired {
        generation,
        safety,
        outer: Rect {
            x: 10,
            y: 10,
            w: 100,
            h: 80,
        },
        style_px: (3, 0, 6),
        color: (0x2a, 0x82, 0xda),
        accent: None,
        use_theme: false,
        target_token: "token-7001".to_owned(),
        target_hwnd: 7001,
        lifetime: test_lifetime(),
        foreground: test_lifetime(),
        fulls: vec![Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }],
        fullscreen_bypass: false,
        topmost: false,
    })
}

/// Stall the input sender exactly like a wedged pump, then drive the
/// suspend dual handoff: the suspend clear absorbs infallibly while
/// stalled, every carrier hide bumps the shared epoch and drops the
/// queued show, and the flush drains FIFO once the pump releases.
#[test]
fn suspend_clears_and_hides_leave_nothing_presentable() {
    let inbox = Arc::new(Inbox::new());
    for _ in 0..INBOX_CAP {
        inbox.push(InboxItem::ClearEsc).expect("fits to capacity");
    }
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    sender.offer(InboxItem::ClearEsc).expect("buffered");
    assert!(sender.admission_closed);

    let render = Arc::new(RenderChannels::new());
    render.post(test_border(1, render.safety()));
    let safety_before = render.safety();
    for carrier in RenderCarrier::all() {
        render.hide(carrier, 1);
    }
    assert_eq!(render.safety(), safety_before + 3);
    // The queued pre-hide show is dropped at the transport edge on
    // every carrier; no acknowledgement is fabricated.
    for carrier in RenderCarrier::all() {
        assert!(render.take_desired(carrier).is_none());
        assert!(render.take_ack(carrier).is_none());
    }

    // The renderer proves the hides with no new owner transition.
    let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&render));
    core.step();
    assert!(core.present().shown.is_empty());
    for carrier in RenderCarrier::all() {
        let ack = render.take_ack(carrier).expect("hide ack");
        assert_eq!(ack.outcome, crate::render_handoff::RenderOutcome::Hidden);
        assert!(ack.hidden_proven);
    }

    // Release the stall: the real pump drains, then the sender flush
    // moves the buffered suspend clear and reopens admission.
    assert_eq!(inbox.drain().len(), INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    assert_eq!(inbox.drain(), vec![InboxItem::ClearEsc]);
}

/// A stalled suspend clear retries behind the stall while render bursts
/// coalesce: the buffered control is never dropped, and three posted
/// frames present exactly once with the newest generation.
#[test]
fn buffered_clear_retries_behind_stall_while_frames_coalesce() {
    let inbox = Arc::new(Inbox::new());
    for _ in 0..INBOX_CAP {
        inbox.push(InboxItem::ClearEsc).expect("fits to capacity");
    }
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    sender.offer(InboxItem::ClearEsc).expect("buffered");
    sender.offer(InboxItem::ClearEsc).expect("deduped");
    assert_eq!(sender.deduped, 1);
    assert_eq!(sender.safety_len(), 0);

    let render = Arc::new(RenderChannels::new());
    render.post(test_border(1, render.safety()));
    render.post(test_border(2, render.safety()));
    render.post(test_border(3, render.safety()));
    let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&render));
    core.step();
    assert_eq!(core.present().shown.len(), 1);
    let ack = render.take_ack(RenderCarrier::Border).expect("present ack");
    assert_eq!(ack.generation, 3);

    // Drain order behind the stall: the buffered suspend clear retries.
    assert_eq!(inbox.drain().len(), INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert_eq!(inbox.drain(), vec![InboxItem::ClearEsc,]);
}

/// Long deterministic stage-3 stress (3000 rounds) over the real
/// transports and both real cores (no TileLoop, no threads, no native
/// calls): variable owner/pump/render stalls, inbox saturation with
/// fixed-bound slots, outbox backpressure with parked retries,
/// render burst coalescing with hide-epoch before/after-take races, and
/// a fault matrix over the real fake seams (hook install fail/retry,
/// renderer create/hide/destroy failure, unclean teardown with a missing
/// then late ack). Per-round invariants: fixed slot bounds, FIFO order,
/// monotonic generations/acks, no presentable stale show. Transport-level
/// offers only (hand-built items with owner-style flush/take/notify);
/// no owner policy logic is duplicated.
#[test]
fn stage3_long_interleaved_stress() {
    use crate::input_handoff::InputSnapshot;
    use crate::input_pump::{FakeBackend, InputChannels, PumpCore, PumpSignals};
    use crate::render_handoff::RenderOutcome;
    use crate::render_pump::Present;
    use crate::render_pump::native::RenderSignals;
    use crate::snapkey::{
        KeyboardConfig, QueuedIntent, QueuedSnapEvent, SnapEdge, SnapOp, SnapOrigin,
    };
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::time::Instant;
    use tiler_core::directional::Direction;

    // Process-global fence guard: applied snapshot generations touch the
    // shared fence. Classifier TLS itself is thread-local and needs no
    // guard; no global Esc edge is touched (no ClearEsc items).
    let _guard = crate::input_handoff::fence_test_serial();

    // Deterministic xorshift64 (fixed seed, no new deps).
    let mut rng: u64 = 0x9E3779B97F4A7C15;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    let snap_event = || {
        QueuedSnapEvent::Intent(QueuedIntent {
            op: SnapOp::Focus,
            direction: Direction::Left,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: Instant::now(),
        })
    };
    let build_snapshot = |generation: u64, take: bool, hwnds: &[u64]| {
        let mut snap_origins = HashMap::new();
        for hwnd in hwnds {
            let origin = SnapOrigin {
                hwnd: *hwnd,
                token: format!("token-{hwnd}"),
                pid: 1000 + *hwnd as u32,
                creation: format!("creation-{hwnd}"),
            };
            snap_origins.insert(*hwnd, origin.clone());
        }
        Arc::new(InputSnapshot {
            generation,
            keyboard: KeyboardConfig {
                takeover: take,
                allow_win_l: false,
            },
            remap: Vec::new(),
            disabled: Vec::new(),
            snap_origins,
            active: take,
            trace: false,
        })
    };

    let pump_channels = Arc::new(InputChannels::new());
    // The sender shares the pump's inbox (like the production bridge):
    // separate FIFOs would never drain.
    let inbox = Arc::clone(&pump_channels.inbox);
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    let (fail_tx, fail_rx) = std::sync::mpsc::sync_channel(16);
    let pump_signals = Arc::new(PumpSignals::new(fail_tx));
    // Install faults up front (keyboard fails twice);
    // teardown faults pre-scripted for the end matrix (5 falses: the
    // keyboard release fails all bounded attempts).
    let mut backend = FakeBackend::new(vec![false, false, true], true);
    backend.unhook_script.clear();
    for _ in 0..5 {
        backend.unhook_script.push_back(false);
    }
    let mut pump = PumpCore::new(
        backend,
        Arc::clone(&pump_channels),
        Arc::clone(&pump_signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    let mut install_events = 0;
    for _ in 0..10 {
        assert!(!pump.step());
        while fail_rx.try_recv().is_ok() {
            install_events += 1;
        }
        if pump_signals.is_ready() {
            break;
        }
        pump.test_force_retry_due();
    }
    assert!(pump_signals.is_ready(), "hooks recover through retries");
    assert!(install_events >= 3, "every install failure is reported");

    // Renderer creation fault up front: refused narrowly, then recovery
    // presents with the exact generation.
    let render = Arc::new(RenderChannels::new());
    let mut fake = FakePresent::new();
    fake.fail_ensure = Some("error: stress create failed".to_owned());
    let mut renderer = RenderCore::new(fake, Arc::clone(&render));
    render.post(test_border(1, render.safety()));
    renderer.step();
    let ack = render.take_ack(RenderCarrier::Border).expect("refused ack");
    assert_eq!(ack.outcome, RenderOutcome::Refused("carrier-lost"));
    renderer.present_mut().fail_ensure = None;
    render.post(test_border(2, render.safety()));
    renderer.step();
    assert_eq!(renderer.present().shown.len(), 1);
    let ack = render.take_ack(RenderCarrier::Border).expect("present ack");
    assert_eq!(ack.generation, 2);
    assert_eq!(ack.outcome, RenderOutcome::Shown);

    const ROUNDS: u64 = 3000;
    let mut published_gen = 0u64;
    let mut posted_gen = 2u64;
    let mut hide_gens = [0u64; 3];
    let mut refused_snaps = 0u64;
    let mut snap_taken = 0u64;
    let mut last_applied = 0u64;
    let mut last_safety = render.safety();

    for round in 0..ROUNDS {
        let r = next();
        // Owner offers (stalled 1/4 of rounds: nothing enters and the
        // retry flush is skipped, so the inbox fills behind the stall).
        if r & 3 != 0 {
            match (r >> 2) % 10 {
                0..=3 => {
                    published_gen += 1;
                    let take = (r >> 6) & 1 == 0;
                    let count = (r >> 8) % 3;
                    let hwnds: Vec<u64> = (0..count).map(|i| 7 + ((r >> (10 + i)) % 3)).collect();
                    match sender.offer(InboxItem::Snapshot(build_snapshot(
                        published_gen,
                        take,
                        &hwnds,
                    ))) {
                        Ok(()) => {}
                        Err(_) => refused_snaps += 1,
                    }
                }
                4..=6 => {
                    sender.offer(InboxItem::ClearEsc).expect("controls absorb");
                }
                _ => {}
            }
            let had = sender.pending_len() + sender.safety_len();
            sender.flush();
            if had > 0 {
                pump_channels.notify_work();
            }
        }
        // Deterministic wedged-pump saturation: fill the inbox, then two
        // policy snapshots while wedged. The inbox is wedged full, so
        // the second snapshot refuses unless an earlier stall already
        // closed admission (then the first refused instead): at least
        // one refusal per fill is guaranteed, proving the fault path
        // fires under real stalls.
        if round % 97 == 0 {
            while inbox.len() < INBOX_CAP {
                inbox.push(InboxItem::ClearEsc).expect("fill");
            }
            published_gen += 1;
            let first = build_snapshot(published_gen, true, &[7]);
            published_gen += 1;
            let second = build_snapshot(published_gen, false, &[]);
            if sender.offer(InboxItem::Snapshot(first)).is_err() {
                refused_snaps += 1;
            }
            if sender.offer(InboxItem::Snapshot(second)).is_err() {
                refused_snaps += 1;
            }
        }
        // Classifier pushes (mirror callbacks; single-round pump stalls
        // can never fill the deep real queue).
        if r & 7 == 0 {
            crate::snapkey::sys::test_push_queued(snap_event());
        }
        // Pump turn (stalled ~1/8 of rounds).
        if r & 7 != 7 {
            assert!(!pump.step());
            let applied = pump_channels.applied_policy_gen.load(Ordering::SeqCst);
            assert!(applied >= last_applied, "apply acks never regress");
            last_applied = applied;
        }
        // Owner drains (skipped ~1/4 of rounds: the outbox saturates and
        // the pump parks its pending retry, woken by the take below).
        if r & 3 != 2
            && let Some(batch) = pump_channels.snap_outbox.take()
        {
            assert!(!batch.items.is_empty());
            snap_taken += 1;
            pump_channels.notify_work();
        }
        // Render burst posts coalesce latest-wins; rotating hides race
        // the take (before-take leg) against the ack (after-take leg).
        if r & 1 == 0 {
            posted_gen += 1;
            render.post(test_border(posted_gen, render.safety()));
        }
        if r % 11 == 0 {
            let carrier = RenderCarrier::all()[(r / 11 % 3) as usize];
            let _pre = render.take_desired(carrier);
            hide_gens[carrier.index()] += 1;
            let (echoed, _) = render.hide(carrier, hide_gens[carrier.index()]);
            assert_eq!(echoed, hide_gens[carrier.index()]);
        }
        // Hide-failure fault in the middle: unverified, then recovery
        // without any new owner transition (one bounded 60ms wait for
        // the 50ms backoff).
        if round == 1500 {
            renderer
                .present_mut()
                .fail_hides
                .insert(RenderCarrier::Border);
            let _pre = render.take_desired(RenderCarrier::Border);
            hide_gens[0] += 1;
            render.hide(RenderCarrier::Border, hide_gens[0]);
            renderer.step();
            let ack = render
                .take_ack(RenderCarrier::Border)
                .expect("hide-fail ack");
            assert_eq!(ack.outcome, RenderOutcome::Unknown("hide-failed"));
            assert!(!ack.hidden_proven);
            std::thread::sleep(std::time::Duration::from_millis(60));
            renderer.step();
            let ack = render
                .take_ack(RenderCarrier::Border)
                .expect("recovery ack");
            assert_eq!(ack.outcome, RenderOutcome::Hidden);
            assert!(ack.hidden_proven);
            assert_eq!(ack.generation, hide_gens[0]);
        }
        let shown_before = renderer.present().shown.len();
        if r % 3 != 0 {
            renderer.step();
            assert!(
                renderer.present().shown.len() - shown_before <= 1,
                "bursts coalesce to at most one present"
            );
            for carrier in RenderCarrier::all() {
                while let Some(ack) = render.take_ack(carrier) {
                    if ack.hidden_proven {
                        assert_eq!(
                            ack.generation,
                            hide_gens[carrier.index()],
                            "hide ack pairs its epoch"
                        );
                    } else if ack.outcome == RenderOutcome::Shown {
                        assert_eq!(
                            carrier,
                            RenderCarrier::Border,
                            "only the posted carrier shows"
                        );
                        assert_eq!(
                            ack.generation, posted_gen,
                            "latest-wins presents the newest frame"
                        );
                    }
                }
            }
        }
        // Per-round transport invariants.
        assert!(sender.safety_len() <= 1);
        assert!(inbox.len() <= INBOX_CAP);
        let safety = render.safety();
        assert!(safety >= last_safety, "safety epoch never regresses");
        last_safety = safety;
    }
    assert!(refused_snaps >= 30, "wedged fills faulted snapshots");
    assert!(sender.faults >= 1, "stalls faulted the transport");

    // Converge: bounded drain of every queue, outbox, pending slot and
    // render ack with no new work entering.
    let mut quiet = 0;
    for _ in 0..2000 {
        let had = sender.pending_len() + sender.safety_len();
        sender.flush();
        if had > 0 {
            pump_channels.notify_work();
        }
        assert!(!pump.step());
        let mut active = had > 0 || !inbox.is_empty();
        if pump_channels.snap_outbox.take().is_some() {
            snap_taken += 1;
            active = true;
            pump_channels.notify_work();
        }
        renderer.step();
        for carrier in RenderCarrier::all() {
            if render.take_ack(carrier).is_some() {
                active = true;
            }
            if render.take_desired(carrier).is_some() {
                active = true;
            }
        }
        if active {
            quiet = 0;
        } else {
            quiet += 1;
            if quiet >= 3 {
                break;
            }
        }
    }
    assert_eq!(sender.pending_len(), 0, "nothing parked");
    assert_eq!(sender.safety_len(), 0, "no slot retained");
    assert!(!sender.admission_closed, "admission reopened");
    assert!(inbox.is_empty(), "inbox drained in order");
    assert!(!pump_channels.snap_outbox.occupied());
    assert!(snap_taken > 100);
    assert_eq!(
        pump_channels.applied_policy_gen.load(Ordering::SeqCst),
        published_gen,
        "every published policy applied"
    );

    // Teardown matrix: input stop observed promptly; the pre-scripted
    // failing release reports unclean; a missing ack reads None (never
    // fabricated) and the late ack reads back exactly.
    pump_channels.stop_flag.store(true, Ordering::SeqCst);
    pump_channels.notify_work();
    assert!(pump.step(), "stop observed promptly");
    assert_eq!(pump_signals.stopped(), None, "missing ack never fabricated");
    let report = pump.shutdown();
    assert!(!report.clean && !report.unhook_ok);
    pump_signals.note_stopped(report.unhook_ok);
    assert_eq!(pump_signals.stopped(), Some(false));
    // Render teardown: failed destroy reports false, recovery destroys;
    // same missing-then-late ack contract.
    let (render_fail_tx, _render_fail_rx) = std::sync::mpsc::sync_channel(8);
    let render_signals = Arc::new(RenderSignals::new(render_fail_tx));
    assert_eq!(render_signals.stopped(), None);
    renderer.present_mut().destroy_ok = false;
    assert!(!renderer.present_mut().destroy_all());
    render_signals.note_stopped(false);
    assert_eq!(render_signals.stopped(), Some(false));
    renderer.present_mut().destroy_ok = true;
    assert!(renderer.present_mut().destroy_all());

    while fail_rx.try_recv().is_ok() {}
    crate::snapkey::sys::test_clear_state();
}
