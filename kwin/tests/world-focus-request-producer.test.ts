// Genuine-producer retained focus requests (v4): for each of the four
// typed causes (explicit-command, automatic-admission, positive-close,
// focus-follows-cursor) the issuing plus three unchanged re-emissions are
// minted by actual core report ingestion through the real
// `broker_world_eval` broker (never synthesized here); all four carry one
// identical focusRequest row while the reply revision advances. The
// issuing and both intermediate publications are SKIPPED (never passed to
// applyWorldReply); only the latest applies, with exactly one native
// setter and feedback carrying the immutable issuing rev (different from
// the latest publication rev). Two redeliveries of the latest stay
// silent. The SAME latest-only consumer then settles end to end: the
// actual retained feedback rows plus the stateful mock backend facts are
// synced to the genuine fake native, a fresh report is built via the
// genuine production capture (`captureWorldObservation`) and encoder
// (`buildWorldReportV4InnerJson`), and the core ingests it and retires
// the request (focusRequest:null). The production entry never receives
// the issuing or intermediate replies (its dispatch stays unanswered).
// No live KWin; broker binaries run through the suite's execution-only
// launcher, never `cargo run`.
//
// Cause keying is by the typed core cause string only, never inferred
// from id prefixes. The cursor lane that enables the focus-follows-cursor
// leg is observation-side only: the settings revision (focusFollowsCursor)
// rides the genuine DiscoverWorld settings lane and the protocol
// `cursorHit` fact rides the genuine producer capture (the encoder emits
// the captured `cursorWindow` windowAt identity verbatim as `cursorHit`
// alongside the raw lane) into the core; the raw lane is validated and
// stripped broker-side while `cursorHit` passes through. The FOCUS
// REQUEST itself is minted by the core, never hand-built. Consumer
// observations are built from the genuine inner-report bytes of the
// intercepted dispatch (native window homing/rects, native order/current,
// focused/foregroundKnown, screens); the focus row's logical scope is
// translated through the accepted reply's own workspace bindings, never
// by logical/native string equality, and an unbound scope fails closed.
// Gap/outer stay the adapter constants, presentation stays the disabled
// lane.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { captureWorldObservation } from "../src/world-observe";
import {
    buildCaptureEnvelope,
    buildWorldReportV4InnerJson,
    isCoreWorldReply,
    parseWorldReply,
} from "../src/world-report";
import {
    awaitQueueCount,
    deskById,
    discover,
    makeHarness,
    truthfulClientFor,
    winById,
    type FakeDesktop,
    type FakeWindow,
    type Harness,
} from "./world-producer-harness";

type Obs = ApplyEnv["observation"];

// Disabled observation presentation lane (adapter constants: no
// foreground styling, no overlays, empty pager).
const DISABLED_OBS_PRESENTATION: Obs["presentation"] = {
    foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false,
    hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false,
    borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "",
    paused: false, pagerOrder: [], pagerFrames: [],
};

// Genuine inner-report row shapes the consumer observation is built from
// (probe-env verified against the intercepted dispatch bytes).
interface GenuineReportWindow {
    win: string;
    out: string;
    rect: [number, number, number, number];
    minimized: boolean;
    hostHidden: boolean;
    hints: { minW: number | null; minH: number | null; maxW: number | null; maxH: number | null };
    nativeWs: string | null;
    fullscreen: boolean;
    maximized: boolean;
    sticky: boolean;
}

interface GenuineReport {
    windows: GenuineReportWindow[];
    native: Array<{ out: string; order: string[]; current: string; occupancy?: Array<{ native: string; count: number | null }> }>;
    focused: string;
    foregroundKnown: boolean;
    gesture: boolean;
    cursor: [number, number] | null;
    screens: Array<{ out: string; full: [number, number, number, number]; work: [number, number, number, number]; complete: boolean }>;
}

function genuineReportOf(payload: string): GenuineReport {
    const envelope = JSON.parse(payload) as Record<string, unknown>;
    const report = envelope["report"] as Record<string, unknown>;
    assert.ok(report !== undefined && typeof report === "object", "dispatch envelope carries the genuine inner report");
    return {
        windows: report["windows"] as GenuineReportWindow[],
        native: report["native"] as GenuineReport["native"],
        focused: report["focused"] as string,
        foregroundKnown: report["foregroundKnown"] as boolean,
        gesture: report["gesture"] as boolean,
        cursor: (report["cursor"] as [number, number] | null) ?? null,
        screens: report["screens"] as GenuineReport["screens"],
    };
}

function screenRect(report: GenuineReport, out: string): { x: number; y: number; w: number; h: number } {
    const row = report.screens.find((s) => s.out === out);
    const tuple = row?.work ?? row?.full ?? [0, 0, 800, 600];
    return { x: tuple[0], y: tuple[1], w: tuple[2], h: tuple[3] };
}

// Consumer observation from genuine inner-report bytes only: every window
// row carries its NATIVE homing (never the logical scope), topology order
// and current come from the native lane, and the foreground stamps come
// from the report's own focused/foregroundKnown.
function consumerObs(report: GenuineReport, refs: Map<string, object>): Obs {
    // Genuine precondition for this proof: every subject homes to exactly
    // one native desktop, with no minimized or host-hidden lifetimes.
    for (const w of report.windows) {
        assert.ok(typeof w.nativeWs === "string" && w.nativeWs !== "", `genuine report homes ${w.win} to one native desktop`);
        assert.equal(w.minimized, false, `genuine report carries ${w.win} unminimized`);
        assert.equal(w.hostHidden, false, `genuine report carries ${w.win} host-shown`);
    }
    const primary = report.native[0]?.out ?? "out";
    return {
        mode: "per-output-local",
        topology: report.native.map((n, index) => ({
            out: n.out, order: [...n.order], current: n.current, complete: true,
            rect: screenRect(report, n.out), primary: index === 0,
        })),
        bindings: [],
        domains: report.native.map((n) => {
            const rows = report.windows.filter((w) => w.out === n.out);
            return {
                id: n.out, ws: n.current, bounds: screenRect(report, n.out),
                gap: 8, outer: 0,
                focused: rows.some((w) => w.win === report.focused) ? report.focused : "",
                readable: true, hold: "",
                complete: true,
                windows: rows.map((w) => ({
                    win: w.win, ref: refs.get(w.win) as object, out: w.out, ws: w.nativeWs as string,
                    rect: { x: w.rect[0], y: w.rect[1], w: w.rect[2], h: w.rect[3] },
                    floating: false, fitExcluded: false,
                    fullscreen: w.fullscreen, maximized: w.maximized, sticky: w.sticky,
                    fixedAuto: false, fixedSuppress: false, nativeWs: w.nativeWs,
                    hints: { ...w.hints },
                    minimized: w.minimized,
                })),
                closed: [],
            };
        }),
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: report.gesture,
        focusedGlobal: report.focused,
        foregroundKnown: report.foregroundKnown,
        screens: report.screens.map((s, index) => ({
            out: s.out,
            full: { x: s.full[0], y: s.full[1], w: s.full[2], h: s.full[3] },
            work: { x: s.work[0], y: s.work[1], w: s.work[2], h: s.work[3] },
            gap: 8, outer: 0, complete: s.complete, primary: s.out === primary && index === 0,
            manufacturer: null, model: null, serial: null,
        })),
        windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true,
        native: report.native.map((n) => ({
            out: n.out, order: [...n.order], current: n.current,
            occupancy: (n.occupancy ?? n.order.map((native) => ({ native, count: null })))
                .map((o) => ({ native: o.native, count: o.count })),
        })),
        cursor: report.cursor === null ? null : { x: report.cursor[0], y: report.cursor[1] },
        actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: DISABLED_OBS_PRESENTATION,
    };
}

async function awaitDispatch(h: Harness, from: number): Promise<number> {
    for (let round = 0; round < 200; round += 1) {
        const index = h.calls.findIndex((call, at) => at >= from && call.method === "DescribeWorld" && !h.answered.has(at));
        if (index >= 0) {
            return index;
        }
        h.runTimers();
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
    assert.ok(false, "expected the genuine capture to dispatch through the production broker");
    throw new Error("unreachable");
}

// Newcomer construction mirrors the harness's own truthful native shape
// (DECOR insets via truthfulClientFor, live frame/client echo setters,
// desktops setter). Admission legs only: no fullscreen/maximize echo.
function attachWindow(
    h: Harness,
    id: string,
    frame: { x: number; y: number; width: number; height: number },
    homeArg?: FakeDesktop,
): FakeWindow {
    const mkSignal = (): {
        handlers: Array<(payload?: unknown) => void>;
        signal: { connect: (handler: (payload?: unknown) => void) => void; disconnect: (handler: (payload?: unknown) => void) => void };
    } => {
        const handlers: Array<(payload?: unknown) => void> = [];
        return {
            handlers,
            signal: {
                connect: (handler: (payload?: unknown) => void): void => {
                    handlers.push(handler);
                },
                disconnect: (handler: (payload?: unknown) => void): void => {
                    const index = handlers.indexOf(handler);
                    if (index >= 0) {
                        handlers.splice(index, 1);
                    }
                },
            },
        };
    };
    const outputChanged = mkSignal();
    const winDesktopsChanged = mkSignal();
    const winHiddenChanged = mkSignal();
    const winFrameChanged = mkSignal();
    const home = homeArg ?? winById(h, "win-a").desktops[0];
    assert.ok(home !== undefined, "existing desktop to home the newcomer");
    const win = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { ...frame },
        clientGeometry: truthfulClientFor(frame),
        output: { name: "out" },
        desktops: [home],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: { width: 100, height: 100 },
        maxSize: { width: 2147483647, height: 2147483647 },
        pid: 1234,
        resourceClass: "Example",
        caption: `Window ${id}`,
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
        outputChanged: outputChanged.signal,
        desktopsChanged: winDesktopsChanged.signal,
        hiddenChanged: winHiddenChanged.signal,
        frameGeometryChanged: winFrameChanged.signal,
        __signals: { outputChanged, desktopsChanged: winDesktopsChanged, hiddenChanged: winHiddenChanged, frameGeometryChanged: winFrameChanged },
    } as unknown as FakeWindow;
    let liveFrame = win.frameGeometry;
    let liveClient = truthfulClientFor(liveFrame);
    Object.defineProperty(win, "frameGeometry", {
        configurable: true,
        get: () => liveFrame,
        set: (rect: { x: number; y: number; width: number; height: number }) => {
            liveFrame = rect;
            liveClient = truthfulClientFor(rect);
            return true;
        },
    });
    Object.defineProperty(win, "clientGeometry", {
        configurable: true,
        get: () => liveClient,
        set: (rect: { x: number; y: number; width: number; height: number }) => {
            liveClient = rect;
            return true;
        },
    });
    let desks = win.desktops;
    Object.defineProperty(win, "desktops", {
        configurable: true,
        get: () => desks,
        set: (refs: FakeWindow["desktops"]) => {
            desks = refs;
            return true;
        },
    });
    h.wins.push(win);
    return win;
}

interface Minted {
    requests: string[];
    raws: string[];
    replies: Array<ApplyEnv["reply"]>;
    revs: string[];
}

// The entry's own dispatch stays unanswered: the adapter never observes
// these publications through its own apply path. All four broker replies
// come from actual report ingestion of the entry's genuine bytes.
async function mintFour(h: Harness, dispatch: number): Promise<Minted> {
    const issuing = h.calls[dispatch]?.payload as string;
    const envelope = JSON.parse(issuing) as Record<string, unknown>;
    const seqBase = Number.parseInt(envelope["sourceSeq"] as string, 10);
    assert.ok(Number.isInteger(seqBase) && seqBase >= 1, "genuine capture carries a canonical sourceSeq");
    const requests: string[] = [issuing];
    const rawReplies: string[] = [];
    rawReplies.push(await h.bridge.send("DescribeWorld", issuing));
    for (let i = 1; i <= 3; i += 1) {
        const clone = {
            ...(envelope as object),
            source: `${envelope["source"] as string}-r${String(i + 1)}`,
            sourceSeq: String(seqBase + i),
        };
        const payload = JSON.stringify(clone);
        requests.push(payload);
        rawReplies.push(await h.bridge.send("DescribeWorld", payload));
    }
    // All four requests are identical except the correlation-only
    // source/sourceSeq increments; the raw report facts never change.
    const stripped = requests.map((payload) => {
        const body = JSON.parse(payload) as Record<string, unknown>;
        const { source, sourceSeq, ...rest } = body;
        void source;
        void sourceSeq;
        return JSON.stringify(rest);
    });
    assert.equal(stripped[1], stripped[0], "first re-emission request identical");
    assert.equal(stripped[2], stripped[0], "second re-emission request identical");
    assert.equal(stripped[3], stripped[0], "third re-emission request identical");
    const replies = rawReplies.map((raw) => {
        const parsed = parseWorldReply(raw);
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "genuine broker reply parses as a core reply");
        if (!isCoreWorldReply(parsed)) {
            throw new Error("unreachable");
        }
        return parsed;
    });
    const revs = replies.map((r) => {
        assert.ok(r.rev !== undefined, "genuine reply carries a revision");
        return r.rev as string;
    });
    assert.ok(BigInt(revs[1] as string) > BigInt(revs[0] as string), "reply rev advances across re-emissions");
    assert.ok(BigInt(revs[2] as string) > BigInt(revs[1] as string), "reply rev advances again");
    assert.ok(BigInt(revs[3] as string) > BigInt(revs[2] as string), "reply rev advances a third time");
    return { requests, raws: rawReplies, replies, revs };
}

// Native binding for one logical scope through the accepted reply's own
// workspace rows (never native string equality or ordinals).
function nativeOf(reply: ApplyEnv["reply"], out: string, logical: string): string | null {
    for (const row of reply.workspaces as Array<{ out: string; id: string; bind: string }>) {
        if (row.out === out && row.id === logical) {
            return row.bind !== "" ? row.bind : null;
        }
    }
    return null;
}

// Latest-only proof plus same-consumer settle for one typed cause: the
// four minted publications carry one identical focusRequest row; the
// consumer applies ONLY the latest against the genuine observation with a
// stateful env; duplicates stay silent; the null envelope retires; then
// the SAME consumer settles via its actual feedback rows and mock backend
// facts through the genuine production capture and encoder, and the core
// retires the request. The production entry never receives the issuing or
// intermediate replies.
async function proveLatestOnly(h: Harness, dispatch: number, minted: Minted, opts: {
    cause: string;
    target: string;
    cmdSeq: string | null;
    expectArrival: boolean;
    extraFocusChecks?: (issued: NonNullable<ApplyEnv["reply"]["focusRequest"]>, latest: ApplyEnv["reply"]) => void;
}): Promise<void> {
    const { replies, revs } = minted;
    const closes = replies.map((r) => r.focusRequest);
    const firstReq = closes[0];
    if (firstReq === null || firstReq === undefined) {
        throw new Error("unreachable");
    }
    assert.ok(closes[0] !== null && closes[1] !== null && closes[2] !== null && closes[3] !== null, "all four genuine publications mint focusRequest");
    assert.deepEqual(closes[1], closes[0], "first re-emission is identical");
    assert.deepEqual(closes[2], closes[0], "second re-emission is identical");
    assert.deepEqual(closes[3], closes[0], "third re-emission is identical");
    const issued = firstReq;
    assert.equal(issued.cause, opts.cause, "typed core cause, never inferred");
    assert.equal(issued.target, opts.target, "frozen target exact");
    assert.equal(issued.cmdSeq ?? null, opts.cmdSeq, "command correlation exact");
    if (opts.expectArrival) {
        assert.ok(issued.afterShown !== null && issued.afterShown !== undefined, "selection-backed request carries frozen arrival deps");
    } else {
        assert.ok(issued.afterShown === null || issued.afterShown === undefined, "request without selection backing carries no arrival deps");
    }
    opts.extraFocusChecks?.(issued, replies[3] as ApplyEnv["reply"]);
    assert.equal(issued.rev, revs[0], "focus rev is the immutable issuing rev");
    assert.notEqual(issued.rev, revs[3], "issuing rev differs from the latest publication rev");
    // Consumer: a fresh adapter that never observed the issuing or
    // intermediate publications applies ONLY the latest.
    const latest = replies[3] as ApplyEnv["reply"];
    const state = new WorldAdapterState(latest.adapter, "owner-a");
    assert.ok(state.onDiscovery(latest.gen, latest.adapter), "consumer binds the genuine generation");
    state.invalidate();
    const flight = state.beginCapture(latest.source, null);
    assert.ok(flight !== null, "consumer opens the genuine source flight");
    assert.deepEqual(
        state.acceptReply(latest, latest.source, state.currentInvalid),
        { applied: true, reason: "accepted" },
    );
    const wins = (latest.windows as Array<{ win: string; out: string; ws: string }>).map((w) => ({
        win: w.win, out: w.out, ws: w.ws,
    }));
    const report = genuineReportOf(minted.requests[3] as string);
    assert.deepEqual(
        wins.map((w) => w.win).sort(),
        report.windows.map((w) => w.win).sort(),
        "consumer window set exact against the genuine report",
    );
    const focusRow = latest.focusRequest;
    assert.ok(focusRow !== null, "genuine focus row present on the latest publication");
    // Consumer refs are the actual fake-native lifetimes, so the stateful
    // env below and the later genuine settlement capture share one truth.
    const liveRefs = new Map<string, object>();
    for (const w of wins) {
        const live = winById(h, w.win) as unknown as object;
        assert.ok(live !== undefined, `fake native carries ${w.win}`);
        liveRefs.set(w.win, live);
    }
    const observation = consumerObs(report, liveRefs);
    const refs = new Map<string, object>();
    for (const domain of observation.domains) {
        for (const w of domain.windows) {
            refs.set(w.win, w.ref);
        }
    }
    // Stateful env: live home/rect/view maps seeded genuine, truthful
    // mutations, focus records and flips the foreground, visibility reads
    // the positive hit-test shape (the exact listed lifetime among all
    // matches, mirroring real-KWin `windowAt` with a negative count).
    const fg: { current: string | null } = { current: report.focused };
    const focusCalls: string[] = [];
    const appliedRevs: string[] = [];
    const liveHome = new Map<string, { out: string; ws: string }>();
    const liveRect = new Map<string, { x: number; y: number; w: number; h: number }>();
    for (const w of report.windows) {
        liveHome.set(w.win, { out: w.out, ws: w.nativeWs as string });
        liveRect.set(w.win, { x: w.rect[0], y: w.rect[1], w: w.rect[2], h: w.rect[3] });
    }
    const liveView = new Map<string, string>();
    for (const n of report.native) {
        liveView.set(n.out, n.current);
    }
    const desks = new Map<string, object>();
    const outs = new Map<string, object>();
    for (const n of report.native) {
        outs.set(n.out, {});
        for (const id of n.order) {
            if (!desks.has(id)) {
                desks.set(id, {});
            }
        }
    }
    const idOf = (ref: object): string | null => {
        for (const [id, r] of refs) {
            if (r === ref) {
                return id;
            }
        }
        return null;
    };
    const countingApply = (reply: ApplyEnv["reply"]): ReturnType<typeof applyWorldReply> => {
        appliedRevs.push(reply.rev);
        return applyWorldReply({
            state,
            observation,
            reply,
            dispatchWatermark: state.currentInvalid,
            takeResult: null,
            log: () => undefined,
            isStillListed: (ref) => idOf(ref) !== null,
            readActiveWindowId: () => fg.current,
            readWindowId: (ref) => idOf(ref),
            readLiveRect: (ref) => {
                const id = idOf(ref);
                return id === null ? null : liveRect.get(id) ?? null;
            },
            readLiveMembership: (ref) => {
                const id = idOf(ref);
                const home = id === null ? undefined : liveHome.get(id);
                return home === undefined ? null : { ...home };
            },
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: true }),
            setGeometry: (ref, rect) => {
                const id = idOf(ref);
                if (id === null) {
                    return false;
                }
                liveRect.set(id, { ...rect });
                return true;
            },
            setMembership: () => { throw new Error("no membership here"); },
            sendToOutput: () => { throw new Error("no send here"); },
            resolveOutput: (out) => outs.get(out) ?? null,
            resolveDesktop: (ws) => desks.get(ws) ?? null,
            switchView: (desktopRef, outRef) => {
                let native: string | null = null;
                let out: string | null = null;
                for (const [id, r] of desks) {
                    if (r === desktopRef) {
                        native = id;
                    }
                }
                for (const [name, r] of outs) {
                    if (r === outRef) {
                        out = name;
                    }
                }
                if (native === null || out === null) {
                    return false;
                }
                liveView.set(out, native);
                return true;
            },
            createDesktop: () => null,
            removeDesktop: () => false,
            focusWindow: (ref) => {
                const id = idOf(ref);
                if (id === null) {
                    return false;
                }
                focusCalls.push(id);
                fg.current = id;
                return true;
            },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (id) => [...liveView.values()].includes(id),
            readLiveView: (out) => liveView.get(out) ?? null,
            desktopById: (id) => desks.get(id) ?? null,
            minDesktops: 2,
        });
    };
    // SKIP by construction: the issuing and both intermediate replies
    // (indices 0, 1, 2) are never passed to applyWorldReply; only the
    // latest (index 3) applies.
    const first = countingApply(latest);
    assert.equal(first.effects, 1, "latest publication dispatches exactly once");
    assert.deepEqual(focusCalls, [issued.target], "one native setter names the frozen target");
    assert.deepEqual(appliedRevs, [revs[3]], "only the latest publication reached apply");
    const rows = state.focusFeedbacksForReport();
    assert.equal(rows.length, 1, "one raw feedback row");
    assert.equal(rows[0]?.id, issued.id, "feedback id preserved");
    assert.equal(rows[0]?.target, issued.target, "feedback target preserved");
    assert.equal(rows[0]?.rev, issued.rev, "feedback carries the immutable issuing rev, not the publication rev");
    assert.equal(rows[0]?.setter, "accepted", "truthful raw setter");
    assert.equal(rows[0]?.reason, "", "ordinary call carries no reason");
    assert.equal(rows[0]?.pre, report.focused, "truthful raw pre from the genuine report");
    assert.equal(rows[0]?.post, issued.target, "truthful raw post");
    assert.equal(rows[0]?.foregroundProven, true, "fresh post proves the target");
    assert.equal(rows[0]?.attemptMs, latest.clock, "write stamp is the latest broker clock");
    if (opts.expectArrival) {
        const members = (issued.afterShown as { members: Array<{ win: string }> }).members.map((m) => m.win);
        assert.deepEqual((rows[0]?.arrival ?? []).map((b) => b.win), members, "arrival samples exactly the frozen members");
        for (const b of rows[0]?.arrival ?? []) {
            assert.equal(b.obs, latest.obs, "bracket obs is the latest reply obs");
        }
    } else {
        assert.deepEqual(rows[0]?.arrival ?? [], [], "request without afterShown rides empty arrival");
    }
    // Two unchanged redeliveries of the latest stay silent.
    const second = countingApply(latest);
    assert.equal(second.effects, 0, "first redelivery is duplicate-spent");
    const third = countingApply(latest);
    assert.equal(third.effects, 0, "second redelivery is duplicate-spent");
    assert.deepEqual(focusCalls, [issued.target], "still exactly one native setter overall");
    assert.equal(state.focusFeedbacksForReport().length, 1, "still one feedback row");
    // Null envelope retires through the actual accounting path. The
    // retained feedback rows survive retirement (they ride until the core
    // incorporates them).
    assert.equal(state.observeFocusEnvelope(null, latest.gen), "absent", "null retires");
    assert.ok(!state.isCurrentFocusRequest(issued, latest.gen), "retired record no longer current");
    const actualFeedback = state.focusFeedbacksForReport();
    assert.equal(actualFeedback.length, 1, "settlement carries the actual feedback row");
    // Sync the genuine fake native to the mock setter results so the
    // settlement capture truthfully observes the post-setter facts. This
    // is an honest capture of the mock setter result, never an invented
    // outcome: the foreground names the spent target and the view names
    // the accepted reply's own native binding.
    try {
        (h.workspace as { activeWindow: unknown }).activeWindow = winById(h, issued.target);
    } catch (error) {
        void error;
        h.activeRef = winById(h, issued.target);
    }
    const native = nativeOf(latest, issued.out, issued.ws);
    if (native !== null) {
        try {
            (h.workspace as { setCurrentDesktopForScreen: (desk: unknown, screen: unknown) => void })
                .setCurrentDesktopForScreen(deskById(h, native), { name: issued.out });
        } catch (error) {
            void error;
        }
    }
    assert.equal(h.activeRef?.internalId, issued.target, "fake native foreground names the spent target");
    // Same-consumer settle: a fresh report built via the genuine
    // production capture and encoder carries the actual feedback rows and
    // the truthful target observation to the core, which retires the
    // request. The production entry never receives the issuing or
    // intermediate replies: its dispatch stays unanswered.
    const bindMap = new Map<string, { logical: string; out: string }>();
    for (const row of (latest.workspaces as Array<{ out: string; id: string; bind: string }>)) {
        if (row.bind !== "") {
            bindMap.set(`${row.out}\u0000${row.bind}`, { logical: row.id, out: row.out });
        }
    }
    const settleObs = captureWorldObservation({
        workspace: h.workspace,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 0,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId, out) => {
            if (out !== undefined) {
                const hit = bindMap.get(`${out}\u0000${nativeId}`);
                return hit === undefined
                    ? null
                    : { logical: hit.logical, out: hit.out, pin: false, disp: false, managed: true };
            }
            for (const [key, value] of bindMap) {
                if (key.endsWith(`\u0000${nativeId}`)) {
                    return { logical: value.logical, out: value.out, pin: false, disp: false, managed: true };
                }
            }
            return null;
        },
        isFloating: () => false,
    });
    assert.ok(settleObs !== null, "genuine settlement capture reads");
    assert.equal(settleObs.focusedGlobal, issued.target, "truthful target observation reaches the core");
    assert.equal(settleObs.foregroundKnown, true, "settlement proves the foreground");
    const inner = buildWorldReportV4InnerJson({
        observation: settleObs,
        creationAttempts: [],
        frameAttempts: [],
        memberAttempts: [],
        viewAttempts: [],
        clearAttempts: [],
        stateAttempts: [],
        focusFeedbacks: actualFeedback,
        intentRecords: [],
    });
    assert.ok(inner !== null, "genuine production encoder spells the settlement report");
    const innerValue = JSON.parse(inner) as Record<string, unknown>;
    assert.equal(innerValue["focused"], issued.target, "settlement bytes carry the truthful target");
    assert.equal(
        innerValue["cursorHit"] ?? null,
        innerValue["cursorWindow"] ?? null,
        "cursorHit mirrors the captured windowAt fact",
    );
    const sentFeedback = innerValue["focusFeedback"] as Array<Record<string, unknown>>;
    assert.equal(sentFeedback.length, 1, "settlement bytes carry the actual feedback row");
    assert.equal(sentFeedback[0]?.["id"], issued.id, "settled feedback id exact");
    assert.equal(sentFeedback[0]?.["target"], issued.target, "settled feedback target exact");
    assert.equal(sentFeedback[0]?.["rev"], issued.rev, "settled feedback rev is the issuing rev");
    assert.equal(sentFeedback[0]?.["setter"], "accepted", "settled feedback setter truthful");
    const settleEnvelope = JSON.parse(minted.requests[3] as string) as Record<string, unknown>;
    const settleAdapter = settleEnvelope["adapter"] as string;
    const settleGen = settleEnvelope["gen"] as string;
    const settleInvalid = settleEnvelope["nativeInvalid"] as string;
    const seqBase = Number.parseInt(settleEnvelope["sourceSeq"] as string, 10);
    assert.ok(Number.isInteger(seqBase), "genuine capture carries a canonical sourceSeq");
    assert.ok(typeof settleInvalid === "string", "genuine capture carries the native watermark");
    const settlePayload = buildCaptureEnvelope(
        settleAdapter,
        settleGen,
        `settle-${issued.id}`,
        String(seqBase + 100),
        settleInvalid,
        inner,
    );
    const rawSettle = await h.bridge.send("DescribeWorld", settlePayload);
    const parsedSettle = parseWorldReply(rawSettle);
    assert.ok(parsedSettle !== null && isCoreWorldReply(parsedSettle), "genuine broker reply parses as a core reply");
    if (!isCoreWorldReply(parsedSettle)) {
        throw new Error("unreachable");
    }
    assert.equal(parsedSettle.focusRequest ?? null, null, "retained request retires after delivery");
    // The issuing and intermediate replies never reached any adapter: only
    // the latest publication applied here, and the production entry's own
    // dispatch stays unanswered below.
    assert.ok(
        appliedRevs.length === 3 && appliedRevs.every((rev) => rev === revs[3]),
        "only the latest publication reached apply, redelivered twice",
    );
    assert.ok(!h.answered.has(dispatch), "production entry never received the issuing reply");
    assert.equal(h.replies.get(dispatch), undefined, "no issuing bytes were delivered");
    assert.deepEqual(focusCalls, [issued.target], "still exactly one native setter overall");
}

describe("world retained focus requests genuine producer", () => {
    it("skips the issuing and intermediate positive-close publications; latest applies once with the issuing rev", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            await h.settle();
            // Focus history win-a -> win-c -> win-b, so the MRU survivor
            // excluding the closed win-b is win-c.
            h.activeRef = winById(h, "win-c");
            h.fire("windowActivated");
            h.runTimers();
            await h.settle();
            h.activeRef = winById(h, "win-b");
            h.fire("windowActivated");
            h.runTimers();
            await h.settle();
            // Positive close of the focused lifetime with the host fallback
            // already on win-a (never the frozen winner win-c).
            const at = h.wins.findIndex((w) => w.internalId === "win-b");
            assert.ok(at >= 0, "win-b present before close");
            const [removed] = h.wins.splice(at, 1);
            h.activeRef = winById(h, "win-a");
            const from = h.calls.length;
            h.fire("windowRemoved", removed);
            h.fire("windowActivated");
            h.runTimers();
            const dispatch = await awaitDispatch(h, from);
            assert.equal(h.activeRef?.internalId, "win-a", "close capture carries the win-a fallback");
            const minted = await mintFour(h, dispatch);
            await proveLatestOnly(h, dispatch, minted, {
                cause: "positive-close",
                target: "win-c",
                cmdSeq: null,
                expectArrival: false,
            });
        } finally {
            await h.stop();
        }
    });

    it("skips the issuing and intermediate automatic-admission publications; latest applies once with the issuing rev", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            await h.settle();
            const fresh = attachWindow(h, "win-new", { x: 600, y: 0, width: 200, height: 200 });
            const from = h.calls.length;
            h.fire("windowAdded", fresh);
            h.runTimers();
            const dispatch = await awaitDispatch(h, from);
            const minted = await mintFour(h, dispatch);
            await proveLatestOnly(h, dispatch, minted, {
                cause: "automatic-admission",
                target: "win-new",
                cmdSeq: null,
                expectArrival: false,
            });
        } finally {
            await h.stop();
        }
    });

    it("skips the issuing and intermediate explicit-command publications; latest applies once with the issuing rev", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c", "win-d"], { desktops: ["ws1", "ws2"] });
        try {
            winById(h, "win-c").desktops = [deskById(h, "ws2")];
            winById(h, "win-d").desktops = [deskById(h, "ws2")];
            await discover(h);
            await h.settle();
            const from = h.calls.length;
            h.handle.requestWorkspaceSelect(2);
            await awaitQueueCount(h, "select-numbered", 1);
            const dispatch = await awaitDispatch(h, from);
            const minted = await mintFour(h, dispatch);
            await proveLatestOnly(h, dispatch, minted, {
                cause: "explicit-command",
                target: "win-c",
                cmdSeq: "1",
                expectArrival: true,
                extraFocusChecks: (issued, latest) => {
                    const after = issued.afterShown as {
                        members: Array<{ win: string; out: string; ws: string; frame: [number, number, number, number] | null }>;
                        views: Array<{ out: string; ws: string }>;
                        selected: Array<{ out: string; ws: string }>;
                        departing: Array<{ out: string; ws: string }>;
                    };
                    assert.deepEqual(
                        after.members.map((m) => m.win).sort(),
                        ["win-c", "win-d"],
                        "frozen member set exact",
                    );
                    for (const m of after.members) {
                        assert.equal(m.out, "out", "frozen member output exact");
                        assert.equal(m.ws, issued.ws, "frozen members share the target logical scope");
                        assert.equal(nativeOf(latest, m.out, m.ws), "ws2", "frozen member binds the selected native");
                    }
                    assert.ok(after.selected.length > 0, "frozen selected scopes present");
                    for (const scope of [...after.views, ...after.selected]) {
                        assert.equal(scope.ws, issued.ws, "frozen view/selected scope is the target scope");
                    }
                    assert.ok(
                        after.departing.some((s) => s.ws !== issued.ws),
                        "frozen departing scope differs from the target",
                    );
                },
            });
        } finally {
            await h.stop();
        }
    });

    it("skips the issuing and intermediate focus-follows-cursor publications; latest applies once with the issuing rev", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            const gen = await discover(h);
            await h.settle();
            // Observation-side enablement only: the settings revision rides
            // the genuine DiscoverWorld settings lane and the pointer
            // facts ride the genuine fake native (cursorPos plus a
            // hit-test windowAt) through the production capture and
            // encoder into the core. The focus request itself stays
            // core-minted.
            const settings = JSON.stringify({
                type: "settings", v: 4, adapter: "test-adapter", gen,
                req: "settings-ffc", rev: "2", values: { focusFollowsCursor: true },
            });
            const settingsReply = JSON.parse(await h.bridge.send("DiscoverWorld", settings)) as Record<string, unknown>;
            assert.equal(settingsReply["status"], "accepted", "core accepts the focus-follows-cursor settings");
            // Genuine fake-native pointer facts: the cursor names win-b's
            // seed centre (400, 100) and windowAt hit-tests the live
            // lifetimes, so the genuine capture carries cursor [400,100]
            // with cursorWindow win-b and the encoder mirrors cursorHit
            // win-b. Installed before the dispatch fires and kept through
            // settlement; removed after the leg.
            (h.workspace as Record<string, unknown>)["cursorPos"] = { x: 400, y: 100 };
            (h.workspace as Record<string, unknown>)["windowAt"] = (point: { x: number; y: number }) =>
                h.wins.filter((w) => {
                    const f = w.frameGeometry as { x: number; y: number; width: number; height: number };
                    return point.x >= f.x && point.x < f.x + f.width && point.y >= f.y && point.y < f.y + f.height;
                });
            try {
                const from = h.calls.length;
                h.activeRef = winById(h, "win-a");
                h.fire("windowActivated");
                h.runTimers();
                const dispatch = await awaitDispatch(h, from);
                const minted = await mintFour(h, dispatch);
                await proveLatestOnly(h, dispatch, minted, {
                    cause: "focus-follows-cursor",
                    target: "win-b",
                    cmdSeq: null,
                    expectArrival: false,
                });
            } finally {
                delete (h.workspace as Record<string, unknown>)["windowAt"];
                (h.workspace as Record<string, unknown>)["cursorPos"] = { x: 2, y: 100 };
            }
        } finally {
            await h.stop();
        }
    });
});
