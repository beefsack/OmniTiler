// Genuine-producer admission, close, float, maximize, fullscreen, sticky,
// refusal and
// workspace-selection journeys through the real production entry
// (`startWorldAdapterEntry`) against the retained `broker_world_eval` broker
// (real core via WorldBridge). The fifo-derived harness (`world-producer-harness`)
// two truthful native-shape additions (fake `fullScreenable` plus a
// `setMaximize` echo driving `maximizeMode`) and an opt-in adapter-side
// preimage seam (`withNativePreimage`, default off): real core replies drive
// every expectation through `appliedMatchesReply`, never assumed geometry. Covers: newcomer admission reflow onto a tiled row, close
// survivor reflow, float on/off with retained placement round-trip,
// maximize on/off through the native setter with exact takes and drain
// targets, fullscreen on/off with the exitable native flag, stacked
// fullscreen-over-maximize with maximize retained beneath on exit, unfloat
// onto fresh admission after a sibling close, sticky on/off through the
// native all-desktops flag with exact drain targets and fresh-admission
// sticky-off, orderly sticky restart/off with Planner-lane retention, a
// singleton-move refusal
// terminal, populated-to-populated selection through the production numbered
// route with continued tiling on the selection, empty-workspace selection
// with admission onto it, and rapid-return supersession with no native view
// write. Fractional-scale legs (opt-in `fractionalScale: 1.25` on the shared
// harness: the fake output carries devicePixelRatio 1.25 and fractional
// logical window readbacks as real KWin reports them; logical bounds stay
// identical) prove the adapter ordinary-fact normalization end to end:
// wire scale stays 1 with normalized integer rects (the fractional seed and
// newcomer readbacks, proven on the envelope legs), and admit/close publish
// the same integer allocations as the scale-1 route. Post-actuation natives
// are integers the adapter wrote, so those checks are exact-equality guards,
// not additional rounding proofs. Maximized/fullscreen/sticky legs opt into the adapter-side preimage
// seam (`withNativePreimage`: synchronous empty-seed / stored acks with v1
// exact keys, mirroring the fullscreen-float-toggle fixture) because the
// retained broker serves no preimage lane; the sticky restart leg
// additionally opts into the opaque-intent seam (`withIntentStore`,
// production `floatIntent: true`) with harness-retained snapshots so the
// restart sees the records the first run wrote, as the Planner service
// persists them in production. Core replies, the take lane and
// the native setters stay genuine. Included in `npm test`.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    appliedMatchesReply,
    appliedMatchesReplyNormalized,
    awaitDispatch,
    awaitQueueCount,
    describeCount,
    describeEnvelopeAt,
    deskById,
    discover,
    frameOf,
    lastDescribeReply,
    makeHarness,
    normalizedFrameOf,
    replyFrames,
    takeReadys,
    truthfulClientFor,
    winById,
    type FakeDesktop,
    type FakeWindow,
    type Harness,
} from "./world-producer-harness";

// Newcomer construction mirrors the harness's own truthful native shape
// (DECOR insets via truthfulClientFor, live frame/client echo setters,
// desktops setter): the entry must observe back exactly what the adapter
// sets, so the wiring is identical, not invented. Newcomers deliberately omit
// the harness's `fullScreenable`/`setMaximize` echo: only use them for
// admission and close legs, never for a maximize or fullscreen leg.
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
            h.geometryWrites.push(`${win.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
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

function takeAll(h: Harness): Array<Record<string, unknown>> {
    return h.takeReplies.map((raw) => JSON.parse(raw) as Record<string, unknown>);
}

// Native-state row lookup on a DescribeWorld reply: each window row carries
// at most one row per state property (`maximized`, `fullscreen`).
function stateRow(reply: Record<string, unknown>, win: string, property: string): Record<string, unknown> {
    const rows = reply["windows"] as Array<Record<string, unknown>>;
    const row = rows.find((w) => w["win"] === win);
    assert.ok(row !== undefined, `reply carries a window row for ${win}`);
    const states = row["state"] as Array<Record<string, unknown>>;
    const found = states.find((s) => s["property"] === property);
    assert.ok(found !== undefined, `reply carries a ${property} state row for ${win}`);
    return found;
}

function areaOf(f: { w: number; h: number }): number {
    return f.w * f.h;
}

describe("world genuine-producer admission, close, float and refusal journeys", () => {
    it("admits a newcomer windowAdded onto the tiled row with exact reply frames", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            await h.settle();
            const before = new Map(h.wins.map((w) => [w.internalId, { ...frameOf(w) }]));
            const seed = { x: 600, y: 0, width: 200, height: 200 };
            const fresh = attachWindow(h, "win-c", seed);
            h.fire("windowAdded", fresh);
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "admission capture accepted");
            // Presentation window set exact: the published roster is the
            // live roster, no more and no fewer.
            const ids = ((reply["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(ids, ["win-a", "win-b", "win-c"], "published window set exact");
            appliedMatchesReply(h, reply);
            const frames = replyFrames(reply);
            const newcomer = frames.get("win-c");
            assert.ok(newcomer !== undefined && newcomer.w > 0 && newcomer.h > 0, "newcomer carries a real tiled frame");
            // The core assigns admission geometry: the published newcomer
            // frame is not the seed rect the fake surface arrived with.
            assert.notDeepEqual(newcomer, { x: seed.x, y: seed.y, w: seed.width, h: seed.height }, "admission replaces the seed rect");
            // Reflow locality from the real reply: the focused neighbor
            // splits to host the newcomer (observed real-core output:
            // newcomer lands right of focused win-a) while the far survivor
            // keeps its exact frame.
            assert.notDeepEqual(frameOf(winById(h, "win-a")), before.get("win-a"), "focused neighbor reflows for the newcomer");
            assert.deepEqual(frameOf(winById(h, "win-b")), before.get("win-b"), "far survivor keeps its exact frame");
        } finally {
            await h.stop();
        }
    });

    it("reflows survivors on windowRemoved close with exact reply frames", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            await h.settle();
            const before = new Map(h.wins.map((w) => [w.internalId, { ...frameOf(w) }]));
            const at = h.wins.findIndex((w) => w.internalId === "win-c");
            assert.ok(at >= 0, "win-c present before close");
            const [removed] = h.wins.splice(at, 1);
            h.fire("windowRemoved", removed);
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "close capture accepted");
            const ids = ((reply["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(ids, ["win-a", "win-b"], "published window set exact after close");
            appliedMatchesReply(h, reply);
            // Close releases the allocation and survivors reflow per the
            // real publication: the absorbing neighbor is strictly larger
            // (observed real-core output: win-a absorbs win-c's share) while
            // the far survivor keeps its exact frame. Only strict growth is
            // asserted, never a ratio: ours proportional rescale is TBD.
            const afterA = frameOf(winById(h, "win-a"));
            assert.ok(areaOf(afterA) > areaOf(before.get("win-a") as { w: number; h: number }), "absorbing survivor strictly larger");
            assert.deepEqual(frameOf(winById(h, "win-b")), before.get("win-b"), "far survivor keeps its exact frame");
        } finally {
            await h.stop();
        }
    });

    it("floats and unfloats through production callbacks with exact reply frames", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            await h.settle();
            const tiledBefore = new Map(h.wins.map((w) => [w.internalId, { ...frameOf(w) }]));
            // Float-on through the production callback. The queue filing
            // settles first; only then does the batch capture dispatch (same
            // pump as the fifo engine tests: settle alone never suffices).
            h.handle.requestFloat();
            await awaitQueueCount(h, "toggle-float", 1);
            const filed = h.commandPayloads("toggle-float");
            assert.equal(filed.length, 1, "float files exactly once");
            assert.deepEqual(
                (filed[0]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
                "float keeps the explicit execution-time marker",
            );
            // Float-on carries no rect: the session-retained placement wins,
            // never a recomputed center (kwin/src/world-native.ts float path).
            assert.equal(
                (filed[0]?.body["command"] as Record<string, unknown>)["rect"],
                null,
                "float-on carries no rect",
            );
            await h.flush();
            h.runTimers();
            await h.settle();
            // First-time geometry is the centered 60% work-area fallback
            // (window-state decision; session centered_float_rect over the
            // 1920x400 client-area work area, gap-projected): the real core
            // publishes [389,85,1142,230] and the take result carries the
            // same computed placement.
            const floatTakes = takeReadys(h);
            assert.equal(floatTakes.length, 1, "float settles its own take");
            assert.equal(floatTakes[0]?.["globalCmd"], "1", "float take rides sequence 1");
            const floatResult = floatTakes[0]?.["result"] as Record<string, unknown>;
            assert.equal(floatResult["op"], "toggle-float", "float take carries the toggle result");
            const floatReply = lastDescribeReply(h);
            const floatRows = floatReply["windows"] as Array<Record<string, unknown>>;
            assert.equal(floatRows.find((w) => w["win"] === "win-a")?.["floating"], true, "subject publishes floating");
            assert.equal(floatRows.find((w) => w["win"] === "win-b")?.["floating"], false, "survivors stay tiled");
            assert.equal(floatRows.find((w) => w["win"] === "win-c")?.["floating"], false, "survivors stay tiled");
            appliedMatchesReply(h, floatReply);
            const floatFrame = frameOf(winById(h, "win-a"));
            assert.deepEqual([floatFrame.x, floatFrame.y, floatFrame.w, floatFrame.h], [389, 85, 1142, 230], "float fallback placement exact");
            assert.deepEqual(floatResult["rect"], [389, 85, 1142, 230], "take result carries the computed placement");
            // Sibling reflow from the real publication: the tiled survivors
            // refill the row (win-c absorbs the share; win-b untouched).
            assert.ok(areaOf(frameOf(winById(h, "win-c"))) > areaOf(tiledBefore.get("win-c") as { w: number; h: number }), "refilling survivor strictly larger");
            assert.deepEqual(frameOf(winById(h, "win-b")), tiledBefore.get("win-b"), "far survivor keeps its exact frame");
            // Unfloat leg: a later press (after the apply flight settles;
            // pressing mid-flight fences `world-apply-in-flight`) carries the
            // live float frame, and the member re-tiles per the real reply.
            h.activeRef = winById(h, "win-a");
            h.handle.requestFloat();
            await awaitQueueCount(h, "toggle-float", 2);
            const filed2 = h.commandPayloads("toggle-float");
            assert.equal(filed2.length, 2, "unfloat files exactly once more");
            assert.deepEqual(
                (filed2[1]?.body["command"] as Record<string, unknown>)["rect"],
                [floatFrame.x, floatFrame.y, floatFrame.w, floatFrame.h],
                "unfloat carries the live float frame",
            );
            await h.flush();
            h.runTimers();
            await h.settle();
            const allTakes = takeReadys(h);
            assert.equal(allTakes.length, 2, "both legs settle their own takes");
            assert.deepEqual(allTakes.map((t) => t["globalCmd"]), ["1", "2"], "takes ride fresh sequences 1/2");
            const unfloatReply = lastDescribeReply(h);
            const unfloatRows = unfloatReply["windows"] as Array<Record<string, unknown>>;
            assert.ok(unfloatRows.every((w) => w["floating"] === false), "unfloat returns the member to tile");
            appliedMatchesReply(h, unfloatReply);
            const retiled = frameOf(winById(h, "win-a"));
            assert.notDeepEqual(retiled, floatFrame, "unfloat leaves the float placement");
            // Fresh-admission pixel position is NOT asserted: on these
            // layouts the real core re-tiles win-a onto its pre-float slot,
            // so old-slot restoration is pixel-indistinguishable from fresh
            // admission. Reported as TBD with a discriminating sequence.
        } finally {
            await h.stop();
        }
    });

    it("refuses a singleton directional move with the real core terminal, natives untouched", async () => {
        const h = await makeHarness("win-a", ["win-a"]);
        try {
            await discover(h);
            await h.settle();
            const nativeBefore = h.wins.map((w) => ({ ...frameOf(w) }));
            const n0 = describeCount(h);
            h.handle.requestMove("left");
            await awaitQueueCount(h, "move-directional", 1);
            const moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 1, "singleton move files");
            assert.deepEqual(
                (moves[0]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
                "move keeps the explicit execution-time marker",
            );
            const moveIndex = await awaitDispatch(h);
            assert.equal(describeCount(h), n0 + 1, "one capture drains the filed move");
            await h.flushAt(moveIndex);
            h.runTimers();
            await h.settle();
            // The cheapest determinate refusal: a singleton tiled member has
            // no neighbor (directional.rs SingleRootLeaf), the session
            // refuses PlannerNoop (session/ops/move.rs), and the real core
            // reports failed/operation-refused on the take lane (never the
            // keyboard-resize internal `unchanged`): natives untouched.
            const takes = takeAll(h);
            assert.equal(takes.length, 1, "refusal settles exactly one terminal");
            assert.equal(takes[0]?.["outcome"], "failed", "singleton move settles failed, never ready");
            assert.equal(takes[0]?.["kind"], "operation-refused", "real core reports operation-refused");
            assert.deepEqual(
                h.wins.map((w) => ({ ...frameOf(w) })),
                nativeBefore,
                "refused move writes no native geometry",
            );
            const reply = lastDescribeReply(h);
            assert.equal(reply["result"], null, "actuator lane never carries the terminal result");
            appliedMatchesReply(h, reply);
        } finally {
            await h.stop();
        }
    });

    it("frames-equal oracle fails on a one-pixel difference (negative control)", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            await h.settle();
            const reply = lastDescribeReply(h);
            const tampered = JSON.parse(JSON.stringify(reply)) as Record<string, unknown>;
            const rows = tampered["windows"] as Array<Record<string, unknown>>;
            assert.ok(rows.length > 0, "real reply carries window rows to tamper");
            const frame = (rows[0] as Record<string, unknown>)["frame"] as [number, number, number, number] | null;
            assert.ok(frame !== null, "first row carries a frame to perturb");
            frame[0] += 1;
            // The oracle compares exact native frames per reply row: a
            // single-pixel drift in one row must fail loudly, proving the
            // journey assertions above are not vacuous.
            assert.throws(() => appliedMatchesReply(h, tampered), /native frame of/, "one-pixel drift fails the oracle");
        } finally {
            await h.stop();
        }
    });
});

describe("world genuine-producer maximize and fullscreen journeys", () => {
    // Maximized/fullscreen/sticky legs run through the real `setMaximize` /
    // `fullScreen` / `onAllDesktops` native setters, so the harness opts into the
    // adapter-side preimage seam (`withNativePreimage`): synchronous
    // empty-seed / stored acks with v1 exact keys, mirroring the
    // fullscreen-float-toggle fixture. Core replies, the take lane and the
    // native setters stay genuine. The drain capture (the DescribeWorld that
    // drains the filed batch) is the authoritative observation of a settled
    // state target: its row reads `{auth:"controlled", reason:""}`.
    // Post-actuation captures park the same row as `{auth:"hold",
    // reason:"unreadable"}`: the lane holds the last command's target value
    // without re-arming (no fresh command re-arms it and no further native
    // readback arrives on the fake surface), so a Hold/unreadable row is the
    // expected parked state, never a failure. The Hold rows below record this
    // harness's quiescence (the fake emits no maximizedChanged/fullScreenChanged
    // facts), not a contract that production KWin parks them; fresh native
    // facts would supersede them. Assert settled targets on the drain capture,
    // never on the post-settle reply.
    it("toggles maximize on and off through the native setter with exact takes and drain targets", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { withNativePreimage: true });
        try {
            await discover(h);
            await h.settle();
            const framesBefore = replyFrames(lastDescribeReply(h));
            // Max-on leg through the production callback.
            h.handle.requestMaximize();
            await awaitQueueCount(h, "toggle-maximize", 1);
            const filed = h.commandPayloads("toggle-maximize");
            assert.equal(filed.length, 1, "maximize files exactly once");
            assert.deepEqual(
                (filed[0]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
                "maximize keeps the explicit execution-time marker",
            );
            const drainIndex = await h.flush();
            const drain = JSON.parse(h.replies.get(drainIndex) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(drain, "win-a", "maximized"),
                { auth: "controlled", cmdSeq: "1", property: "maximized", reason: "", value: true },
                "drain capture carries the settled maximize target as Controlled",
            );
            appliedMatchesReply(h, drain);
            assert.deepEqual(replyFrames(drain).get("win-a"), framesBefore.get("win-a"), "maximize-on keeps the tiled frame");
            assert.deepEqual(replyFrames(drain).get("win-b"), framesBefore.get("win-b"), "maximize-on keeps the survivor frame");
            h.runTimers();
            await h.settle();
            const takes = takeReadys(h);
            assert.equal(takes.length, 1, "maximize settles its own take");
            assert.equal(takes[0]?.["globalCmd"], "1", "maximize take rides sequence 1");
            assert.deepEqual(
                takes[0]?.["result"],
                { op: "maximize", win: "win-a", maximized: true, clearFullscreen: false, wasMaximized: false },
                "maximize take carries the shaped toggle result",
            );
            assert.deepEqual(h.nativeStateWrites, ["win-a@setMaximize:true,true"], "maximize-on drives setMaximize(true, true) once");
            assert.equal(winById(h, "win-a").maximizeMode, 3, "maximize-on drives maximizeMode 3");
            const settled = lastDescribeReply(h);
            assert.deepEqual(
                stateRow(settled, "win-a", "maximized"),
                { auth: "hold", cmdSeq: "1", property: "maximized", reason: "unreadable", value: true },
                "post-actuation capture parks the settled target as Hold/unreadable",
            );
            appliedMatchesReply(h, settled);
            // Max-off leg: a later press (after the apply flight settles;
            // pressing mid-flight fences `world-apply-in-flight`).
            h.handle.requestMaximize();
            await awaitQueueCount(h, "toggle-maximize", 2);
            assert.equal(h.commandPayloads("toggle-maximize").length, 2, "maximize-off files exactly once more");
            const drainIndex2 = await h.flush();
            const drain2 = JSON.parse(h.replies.get(drainIndex2) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(drain2, "win-a", "maximized"),
                { auth: "controlled", cmdSeq: "2", property: "maximized", reason: "", value: false },
                "drain capture carries the cleared maximize target as Controlled",
            );
            appliedMatchesReply(h, drain2);
            h.runTimers();
            await h.settle();
            const takes2 = takeReadys(h);
            assert.equal(takes2.length, 2, "both legs settle their own takes");
            assert.deepEqual(takes2.map((t) => t["globalCmd"]), ["1", "2"], "takes ride fresh sequences 1/2");
            assert.deepEqual(
                takes2[1]?.["result"],
                { op: "maximize", win: "win-a", maximized: false, clearFullscreen: false, wasMaximized: true },
                "maximize-off take carries the cleared toggle result",
            );
            assert.deepEqual(
                h.nativeStateWrites,
                ["win-a@setMaximize:true,true", "win-a@setMaximize:false,false"],
                "maximize-off drives setMaximize(false, false) once",
            );
            assert.equal(winById(h, "win-a").maximizeMode, 0, "maximize-off clears maximizeMode");
            assert.deepEqual(
                stateRow(lastDescribeReply(h), "win-a", "maximized"),
                { auth: "hold", cmdSeq: "2", property: "maximized", reason: "unreadable", value: false },
                "post-actuation capture parks the cleared target as Hold/unreadable",
            );
            appliedMatchesReply(h, lastDescribeReply(h));
            assert.deepEqual(frameOf(winById(h, "win-a")), { ...framesBefore.get("win-a") }, "round trip restores the exact tiled frame");
        } finally {
            await h.stop();
        }
    });

    it("toggles fullscreen on and off with the exitable native flag and exact drain targets", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { withNativePreimage: true });
        try {
            await discover(h);
            await h.settle();
            const framesBefore = replyFrames(lastDescribeReply(h));
            const geoBefore = h.geometryWrites.length;
            // Fullscreen-on leg through the production callback.
            h.handle.requestFullscreen();
            await awaitQueueCount(h, "toggle-fullscreen", 1);
            const filed = h.commandPayloads("toggle-fullscreen");
            assert.equal(filed.length, 1, "fullscreen files exactly once");
            assert.deepEqual(
                (filed[0]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
                "fullscreen keeps the explicit execution-time marker",
            );
            const drainIndex = await h.flush();
            const drain = JSON.parse(h.replies.get(drainIndex) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(drain, "win-a", "fullscreen"),
                { auth: "controlled", cmdSeq: "1", property: "fullscreen", reason: "", value: true },
                "drain capture carries the settled fullscreen target as Controlled",
            );
            appliedMatchesReply(h, drain);
            h.runTimers();
            await h.settle();
            const takes = takeReadys(h);
            assert.equal(takes.length, 1, "fullscreen settles its own take");
            assert.equal(takes[0]?.["globalCmd"], "1", "fullscreen take rides sequence 1");
            // Fullscreen takes resolve a null result (the snapshot IS the
            // result); there is no shaped toggle result as maximize carries.
            assert.equal(takes[0]?.["result"], null, "fullscreen take resolves a null result");
            assert.equal(winById(h, "win-a").fullScreen, true, "fullscreen-on sets the native flag");
            assert.equal(h.geometryWrites.length, geoBefore, "fullscreen-on pauses geometry writes to the subject");
            assert.deepEqual(
                stateRow(lastDescribeReply(h), "win-a", "fullscreen"),
                { auth: "hold", cmdSeq: "1", property: "fullscreen", reason: "unreadable", value: true },
                "post-actuation capture parks the settled target as Hold/unreadable",
            );
            appliedMatchesReply(h, lastDescribeReply(h));
            // Fullscreen-off leg exits through the exitable native flag.
            h.handle.requestFullscreen();
            await awaitQueueCount(h, "toggle-fullscreen", 2);
            assert.equal(h.commandPayloads("toggle-fullscreen").length, 2, "fullscreen-off files exactly once more");
            const drainIndex2 = await h.flush();
            const drain2 = JSON.parse(h.replies.get(drainIndex2) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(drain2, "win-a", "fullscreen"),
                { auth: "controlled", cmdSeq: "2", property: "fullscreen", reason: "", value: false },
                "drain capture carries the cleared fullscreen target as Controlled",
            );
            appliedMatchesReply(h, drain2);
            h.runTimers();
            await h.settle();
            const takes2 = takeReadys(h);
            assert.equal(takes2.length, 2, "both legs settle their own takes");
            assert.deepEqual(takes2.map((t) => t["globalCmd"]), ["1", "2"], "takes ride fresh sequences 1/2");
            assert.equal(takes2[1]?.["result"], null, "fullscreen-off take resolves null");
            assert.equal(winById(h, "win-a").fullScreen, false, "fullscreen-off clears the native flag");
            assert.deepEqual(replyFrames(lastDescribeReply(h)).get("win-a"), framesBefore.get("win-a"), "exit restores the exact tiled frame");
            assert.deepEqual(replyFrames(lastDescribeReply(h)).get("win-b"), framesBefore.get("win-b"), "exit keeps the survivor frame");
            appliedMatchesReply(h, lastDescribeReply(h));
        } finally {
            await h.stop();
        }
    });

    it("stacks fullscreen over maximize and exits retaining maximize beneath", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { withNativePreimage: true });
        try {
            await discover(h);
            await h.settle();
            const allocBefore = replyFrames(lastDescribeReply(h));
            // Maximize-on leg first.
            h.handle.requestMaximize();
            await awaitQueueCount(h, "toggle-maximize", 1);
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.equal(winById(h, "win-a").maximizeMode, 3, "maximize-on drives maximizeMode 3");
            // Fullscreen-on over the retained maximize overlay: the core
            // enters immediately with no unmaximize (window-state B-maximize
            // overlay), so no further native maximize write lands.
            const writesAfterMax = [...h.nativeStateWrites];
            h.handle.requestFullscreen();
            await awaitQueueCount(h, "toggle-fullscreen", 1);
            const fsIndex = await h.flush();
            const fsDrain = JSON.parse(h.replies.get(fsIndex) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(fsDrain, "win-a", "fullscreen"),
                { auth: "controlled", cmdSeq: "2", property: "fullscreen", reason: "", value: true },
                "drain capture carries the stacked fullscreen target as Controlled",
            );
            assert.deepEqual(
                stateRow(fsDrain, "win-a", "maximized"),
                { auth: "hold", cmdSeq: "1", property: "maximized", reason: "unreadable", value: true },
                "maximize beneath stays parked as Hold/unreadable while fullscreen actuates",
            );
            appliedMatchesReply(h, fsDrain);
            h.runTimers();
            await h.settle();
            assert.deepEqual(takeReadys(h).map((t) => t["globalCmd"]), ["1", "2"], "both commands settle their own takes");
            assert.equal(winById(h, "win-a").fullScreen, true, "fullscreen-on sets the native flag");
            assert.equal(winById(h, "win-a").maximizeMode, 3, "fullscreen entry keeps maximizeMode 3");
            assert.deepEqual(h.nativeStateWrites, writesAfterMax, "fullscreen entry issues no unmaximize");
            // Fullscreen-off retains the maximize overlay beneath: no extra
            // maximize write, mode stays 3, allocation frames restored.
            h.handle.requestFullscreen();
            await awaitQueueCount(h, "toggle-fullscreen", 2);
            const offIndex = await h.flush();
            const offDrain = JSON.parse(h.replies.get(offIndex) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(offDrain, "win-a", "fullscreen"),
                { auth: "controlled", cmdSeq: "3", property: "fullscreen", reason: "", value: false },
                "drain capture carries the cleared fullscreen target as Controlled",
            );
            assert.deepEqual(
                stateRow(offDrain, "win-a", "maximized"),
                { auth: "hold", cmdSeq: "1", property: "maximized", reason: "unreadable", value: true },
                "maximize beneath stays parked as Hold/unreadable after fullscreen exit",
            );
            appliedMatchesReply(h, offDrain);
            h.runTimers();
            await h.settle();
            assert.deepEqual(takeReadys(h).map((t) => t["globalCmd"]), ["1", "2", "3"], "all three commands settle their own takes");
            assert.equal(takeReadys(h)[2]?.["result"], null, "fullscreen-off take resolves null");
            assert.equal(winById(h, "win-a").fullScreen, false, "fullscreen-off clears the native flag");
            assert.equal(winById(h, "win-a").maximizeMode, 3, "fullscreen exit retains maximizeMode 3");
            assert.deepEqual(h.nativeStateWrites, writesAfterMax, "fullscreen exit issues no extra maximize write");
            assert.deepEqual(replyFrames(lastDescribeReply(h)).get("win-a"), allocBefore.get("win-a"), "exit restores the allocation frame");
            assert.deepEqual(replyFrames(lastDescribeReply(h)).get("win-b"), allocBefore.get("win-b"), "exit keeps the survivor frame");
            appliedMatchesReply(h, lastDescribeReply(h));
        } finally {
            await h.stop();
        }
    });

    it("unfloats onto fresh admission after a sibling close, never old-slot restoration", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"], { withNativePreimage: true });
        try {
            await discover(h);
            await h.settle();
            const preFloatSlot = { ...frameOf(winById(h, "win-a")) };
            // Float win-a, then close win-c: the tiled survivors refill the
            // row while win-a keeps its retained float placement.
            h.handle.requestFloat();
            await awaitQueueCount(h, "toggle-float", 1);
            await h.flush();
            h.runTimers();
            await h.settle();
            const floatFrame = frameOf(winById(h, "win-a"));
            const at = h.wins.findIndex((w) => w.internalId === "win-c");
            assert.ok(at >= 0, "win-c present before close");
            const [removed] = h.wins.splice(at, 1);
            h.fire("windowRemoved", removed);
            h.runTimers();
            await h.settle();
            // Unfloat onto the changed roster: unfloat is fresh ordinary
            // admission, never old-slot restoration (window-state.md
            // float/unfloat), so win-a must NOT return to its pre-float
            // slot once that slot no longer exists on the live roster.
            h.activeRef = winById(h, "win-a");
            h.handle.requestFloat();
            await awaitQueueCount(h, "toggle-float", 2);
            const filed = h.commandPayloads("toggle-float");
            assert.equal(filed.length, 2, "unfloat files exactly once more");
            const unfloatIndex = await h.flush();
            const unfloatDrain = JSON.parse(h.replies.get(unfloatIndex) as string) as Record<string, unknown>;
            h.runTimers();
            await h.settle();
            const allTakes = takeReadys(h);
            assert.equal(allTakes.length, 2, "both legs settle their own takes");
            const unfloatReply = lastDescribeReply(h);
            const unfloatRows = unfloatReply["windows"] as Array<Record<string, unknown>>;
            assert.ok(unfloatRows.every((w) => w["floating"] === false), "unfloat returns the member to tile");
            appliedMatchesReply(h, unfloatDrain);
            appliedMatchesReply(h, unfloatReply);
            const retiled = frameOf(winById(h, "win-a"));
            assert.notDeepEqual(retiled, floatFrame, "unfloat leaves the float placement");
            // The pre-float slot belonged to the 3-window roster; the real
            // core fresh-admits instead, so the retiled frame must differ
            // from the old slot and match the fresh 2-window placement.
            assert.notDeepEqual(retiled, preFloatSlot, "unfloat never restores the pre-float slot");
            assert.deepEqual(retiled, { x: 964, y: 8, w: 948, h: 384 }, "unfloat lands on the fresh 2-window admission placement");
        } finally {
            await h.stop();
        }
    });
});

describe("world genuine-producer sticky journeys", () => {
    // Sticky legs run through the real `onAllDesktops` native setter with
    // the KWin-faithful fake echo (on empties the desktop list, off
    // restores the output current; sibling src/window.cpp
    // `Window::setOnAllDesktops`). Tiled-origin sticky floats first
    // (window-state decision), so the subject leaves the row and the
    // survivor refills; sticky-off is fresh ordinary admission per
    // REQ-FLT-01, never old-slot restoration. Takes resolve a null result
    // (core `command_result` is None for native sticky transitions; only
    // the state target carries the verdict). Drain-capture assertion rule
    // from the maximize/fullscreen legs above applies unchanged.
    it("toggles sticky on and off through the native all-desktops flag with exact drain targets", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { withNativePreimage: true });
        try {
            await discover(h);
            await h.settle();
            const framesBefore = replyFrames(lastDescribeReply(h));
            // Sticky-on through the production callback.
            h.handle.requestSticky();
            await awaitQueueCount(h, "toggle-sticky", 1);
            const filed = h.commandPayloads("toggle-sticky");
            assert.equal(filed.length, 1, "sticky files exactly once");
            assert.deepEqual(
                (filed[0]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
                "sticky keeps the explicit execution-time marker",
            );
            const drainIndex = await h.flush();
            const drain = JSON.parse(h.replies.get(drainIndex) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(drain, "win-a", "sticky"),
                { auth: "controlled", cmdSeq: "1", property: "sticky", reason: "", value: true },
                "drain capture carries the settled sticky target as Controlled",
            );
            appliedMatchesReply(h, drain);
            const stickyRows = drain["windows"] as Array<Record<string, unknown>>;
            assert.equal(stickyRows.find((w) => w["win"] === "win-a")?.["floating"], true, "subject publishes floating");
            assert.equal(stickyRows.find((w) => w["win"] === "win-b")?.["floating"], false, "survivor stays tiled");
            h.runTimers();
            await h.settle();
            const takes = takeReadys(h);
            assert.equal(takes.length, 1, "sticky settles its own take");
            assert.equal(takes[0]?.["globalCmd"], "1", "sticky take rides sequence 1");
            assert.equal(takes[0]?.["result"], null, "sticky take resolves a null result");
            assert.deepEqual(h.nativeStateWrites, ["win-a@onAllDesktops:true"], "sticky-on writes onAllDesktops(true) once");
            assert.equal(winById(h, "win-a").onAllDesktops, true, "sticky-on sets the native flag");
            assert.deepEqual(winById(h, "win-a").desktops.map((d) => d.id), [], "sticky-on empties the native desktop list");
            assert.deepEqual(
                stateRow(lastDescribeReply(h), "win-a", "sticky"),
                { auth: "hold", cmdSeq: "1", property: "sticky", reason: "unreadable", value: true },
                "post-actuation capture parks the settled target as Hold/unreadable",
            );
            appliedMatchesReply(h, lastDescribeReply(h));
            // Tiled-origin sticky floats first (window-state decision) but
            // keeps the observed tile frame as its float placement: core
            // sets member geometry from the observed fact rect
            // (`run_sticky_transition` in crates/tiler-core/src/world_sticky.rs),
            // never the centered first-time fallback the float toggle
            // uses. The lone survivor refills the full single-tile row
            // (gap arithmetic over the 1920x400 client area).
            assert.deepEqual(frameOf(winById(h, "win-a")), framesBefore.get("win-a"), "sticky-on retains the tile frame as the float placement");
            assert.deepEqual(frameOf(winById(h, "win-b")), { x: 8, y: 8, w: 1904, h: 384 }, "lone survivor refills the full row");
            // Sticky-off leg returns the subject to tile via fresh admission.
            h.handle.requestSticky();
            await awaitQueueCount(h, "toggle-sticky", 2);
            assert.equal(h.commandPayloads("toggle-sticky").length, 2, "sticky-off files exactly once more");
            const drainIndex2 = await h.flush();
            const drain2 = JSON.parse(h.replies.get(drainIndex2) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(drain2, "win-a", "sticky"),
                { auth: "controlled", cmdSeq: "2", property: "sticky", reason: "", value: false },
                "drain capture carries the cleared sticky target as Controlled",
            );
            appliedMatchesReply(h, drain2);
            h.runTimers();
            await h.settle();
            const takes2 = takeReadys(h);
            assert.equal(takes2.length, 2, "both legs settle their own takes");
            assert.deepEqual(takes2.map((t) => t["globalCmd"]), ["1", "2"], "takes ride fresh sequences 1/2");
            assert.ok(takes2.every((t) => t["result"] === null), "takes resolve null results");
            assert.deepEqual(
                h.nativeStateWrites,
                ["win-a@onAllDesktops:true", "win-a@onAllDesktops:false"],
                "sticky-off writes onAllDesktops(false) once",
            );
            assert.equal(winById(h, "win-a").onAllDesktops, false, "sticky-off clears the native flag");
            assert.deepEqual(
                winById(h, "win-a").desktops.map((d) => d.id),
                ["ws1"],
                "sticky-off restores the output-current native home",
            );
            assert.deepEqual(
                stateRow(lastDescribeReply(h), "win-a", "sticky"),
                { auth: "hold", cmdSeq: "2", property: "sticky", reason: "unreadable", value: false },
                "post-actuation capture parks the cleared target as Hold/unreadable",
            );
            appliedMatchesReply(h, lastDescribeReply(h));
            // REQ-FLT-01 fresh admission: the two fixtures hold the two
            // base allocations in either order. Old-slot pixel position is
            // NOT asserted: on this 2-window layout the real core
            // fresh-admits win-a onto its pre-sticky slot, so restoration
            // is pixel-indistinguishable from fresh admission (same
            // limitation the float journey records as TBD; the live
            // runner's swapped sticky-off observation is the
            // discriminating counterpart).
            const base = [framesBefore.get("win-a"), framesBefore.get("win-b")].sort((a, b) => (a as { x: number }).x - (b as { x: number }).x);
            const live = [frameOf(winById(h, "win-a")), frameOf(winById(h, "win-b"))].sort((a, b) => a.x - b.x);
            assert.deepEqual(live, base, "sticky-off fresh-admits onto the two base allocations");
        } finally {
            await h.stop();
        }
    });

    it("restores sticky across an orderly stop/start then sticky-off works", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { withNativePreimage: true, withIntentStore: true });
        try {
            await discover(h);
            await h.settle();
            const framesBefore = replyFrames(lastDescribeReply(h));
            h.handle.requestSticky();
            await awaitQueueCount(h, "toggle-sticky", 1);
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.equal(winById(h, "win-a").onAllDesktops, true, "sticky-on sets the native flag");
            assert.equal(takeReadys(h).length, 1, "sticky-on settles its own take");
            // The durability lanes carry the run across the restart: one
            // opaque intent row (sticky save) plus native preimage
            // originals, exactly what production persists Planner-side.
            assert.equal(h.intentRetained.length, 1, "sticky-on persists one opaque intent row");
            assert.equal(h.intentRetained[0]?.win, "win-a", "intent row names the subject");
            assert.ok(h.preimageRetained.length > 0, "sticky-on persists native preimage originals");
            const takesBefore = takeReadys(h).length;
            // Orderly stop then a fresh entry on the SAME fake surface and
            // bridge: the broker resets core on the new handshake (same as
            // a fresh broker process), so only native facts plus the
            // retained Planner lanes carry over.
            const callsBefore = h.calls.length;
            h.restart();
            await discover(h, callsBefore);
            await h.settle();
            assert.deepEqual(winById(h, "win-a").desktops.map((d) => d.id), [], "restart keeps the empty native membership");
            assert.equal(winById(h, "win-a").onAllDesktops, true, "restart keeps the native all-desktops flag");
            const revived = lastDescribeReply(h);
            assert.equal(
                (revived["windows"] as Array<Record<string, unknown>>).find((w) => w["win"] === "win-a")?.["floating"],
                true,
                "revived subject publishes floating",
            );
            appliedMatchesReply(h, revived);
            // Commandless restoration (offline sticky-restart correction):
            // no live command re-arms the lane and no fresh native facts
            // arrive on the fake, so the restored true value parks as
            // Hold/unreadable with a null command correlation, exactly
            // like the post-actuation parks above.
            assert.deepEqual(
                stateRow(revived, "win-a", "sticky"),
                { auth: "hold", cmdSeq: null, property: "sticky", reason: "unreadable", value: true },
                "revived capture restores the sticky target as parked Hold",
            );
            assert.equal(takeReadys(h).length, takesBefore, "restart mints no takes without a new command");
            // Sticky-off after restart honors the saved tiled prior: fresh
            // admission onto the two base allocations in either order.
            h.handle.requestSticky();
            await awaitQueueCount(h, "toggle-sticky", 2);
            const offIndex = await h.flush();
            const offDrain = JSON.parse(h.replies.get(offIndex) as string) as Record<string, unknown>;
            assert.deepEqual(
                stateRow(offDrain, "win-a", "sticky"),
                { auth: "controlled", cmdSeq: "1", property: "sticky", reason: "", value: false },
                "drain capture carries the cleared sticky target as Controlled",
            );
            appliedMatchesReply(h, offDrain);
            h.runTimers();
            await h.settle();
            const takesAfter = takeReadys(h).slice(takesBefore);
            assert.equal(takesAfter.length, 1, "sticky-off settles its own take");
            assert.equal(takesAfter[0]?.["result"], null, "sticky-off take resolves a null result");
            assert.equal(winById(h, "win-a").onAllDesktops, false, "sticky-off clears the native flag");
            assert.deepEqual(
                winById(h, "win-a").desktops.map((d) => d.id),
                ["ws1"],
                "sticky-off restores the output-current native home",
            );
            appliedMatchesReply(h, lastDescribeReply(h));
            const base = [framesBefore.get("win-a"), framesBefore.get("win-b")].sort((a, b) => (a as { x: number }).x - (b as { x: number }).x);
            const live = [frameOf(winById(h, "win-a")), frameOf(winById(h, "win-b"))].sort((a, b) => a.x - b.x);
            assert.deepEqual(live, base, "sticky-off fresh-admits onto the two base allocations");
        } finally {
            await h.stop();
        }
    });
});

describe("world genuine-producer workspace selection journeys", () => {
    // Genuine-producer tier 2A: populated-to-populated selection through the
    // production numbered route, empty-workspace selection with admission
    // onto it, and rapid return before arrival lands. The multi-desktop
    // surface comes from the harness `desktops` option (default stays one
    // native); windows re-home through the live `desktops` setter before
    // discovery. Pump order per leg is the fifo-established
    // `awaitQueueCount` -> `flush`/`awaitDispatch` -> `runTimers` ->
    // `settle`; expectations come from the selection contract
    // (`crates/tiler-core/src/world_selection.rs`, `selection_terminal`),
    // the focusless-completion decision and the v4-phase2 selection
    // evidence, never from first-run output.
    function nativeCurrent(h: Harness): string {
        const read = h.workspace["currentDesktopForScreen"] as (screen: { name: string }) => FakeDesktop;
        return read({ name: "out" }).id;
    }

    function selectedLogical(reply: Record<string, unknown>, nativeId: string): string {
        const rows = reply["workspaces"] as Array<Record<string, unknown>>;
        const row = rows.find((b) => b["bind"] === nativeId);
        assert.ok(row !== undefined, `reply binds the selected native ${nativeId} to a logical id`);
        return row["id"] as string;
    }

    it("selects a populated workspace through the production numbered route and continues tiling on it", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c", "win-d"], { desktops: ["ws1", "ws2"] });
        try {
            winById(h, "win-c").desktops = [deskById(h, "ws2")];
            winById(h, "win-d").desktops = [deskById(h, "ws2")];
            await discover(h);
            await h.settle();
            // Production numbered route: ring position 2 is the ws2 native
            // cell in native order; the command stays neutral (`out: null`)
            // for cursor-first output resolution.
            h.handle.requestWorkspaceSelect(2);
            await awaitQueueCount(h, "select-numbered", 1);
            const filed = h.commandPayloads("select-numbered");
            assert.equal(filed.length, 1, "selection files exactly once");
            assert.deepEqual(
                filed[0]?.body["command"],
                { op: "select-numbered", out: null, index: 2 },
                "selection carries the neutral cursor-first numbered command",
            );
            const drainIndex = await h.flush();
            const drain = JSON.parse(h.replies.get(drainIndex) as string) as Record<string, unknown>;
            // A populated grant resolves an eligible focus target: one
            // `awaitShown` focus grant on the draining capture (each
            // populated grant carries its own command sequence and
            // `awaitShown:true` per the native selection evidence). The
            // grant target must home to the selected cell.
            const effects = drain["effects"] as Array<Record<string, unknown>>;
            assert.equal(effects.length, 1, "populated grant mints one focus grant");
            assert.equal(effects[0]?.["kind"], "focus", "grant is a focus effect");
            assert.equal(effects[0]?.["awaitShown"], true, "grant waits for arrival showing");
            assert.equal(effects[0]?.["cmdSeq"], "1", "grant rides sequence 1");
            assert.equal(effects[0]?.["cause"], "explicit-command", "grant is command-owned");
            const grantTarget = effects[0]?.["target"] as string;
            const drainRows = drain["windows"] as Array<Record<string, unknown>>;
            const grantRow = drainRows.find((w) => w["win"] === grantTarget);
            assert.ok(grantRow !== undefined, "grant names a published member");
            assert.equal(grantRow["ws"], selectedLogical(drain, "ws2"), "grant targets the selected home");
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.viewWrites, ["out->ws2"], "native current moves to the target exactly once");
            assert.equal(nativeCurrent(h), "ws2", "native current reads back the target");
            const takes = takeReadys(h);
            assert.equal(takes.length, 1, "selection settles its own take");
            assert.equal(takes[0]?.["globalCmd"], "1", "take rides sequence 1");
            assert.equal(takes[0]?.["result"], null, "take resolves null: queue/take is report-result delivery, not a switch receipt");
            const reply = lastDescribeReply(h);
            assert.equal(reply["result"], null, "actuator lane never carries the selection terminal");
            // The fake has no `windowAt` shown/arrival witness, so the
            // focus-bearing binding cannot prove focus (`selection_terminal`
            // in world_selection.rs: complete needs no effect or proven
            // focus); every frozen visibility member confirmed, so `failed`
            // stays empty and no proven focus is ever fabricated.
            assert.deepEqual(
                reply["switchSettled"],
                [{ op: "op-1", outcome: "completed-with-failures", focus: "unproven", failed: [], fallback: false }],
                "focus-bearing binding without feedback stays unproven",
            );
            const ids = ((reply["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(ids, ["win-a", "win-b", "win-c", "win-d"], "published window set exact");
            appliedMatchesReply(h, reply);
            // Continued ordinary tiling on the selected workspace: admit a
            // newcomer homed to the selection, then close one member. Each
            // leg republishes exact frames with the terminal never replayed.
            const fresh = attachWindow(h, "win-e", { x: 600, y: 0, width: 200, height: 200 }, deskById(h, "ws2"));
            h.fire("windowAdded", fresh);
            h.runTimers();
            await h.settle();
            const admitted = lastDescribeReply(h);
            const admIds = ((admitted["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(admIds, ["win-a", "win-b", "win-c", "win-d", "win-e"], "published window set exact after admission");
            const newcomer = (admitted["windows"] as Array<Record<string, unknown>>).find((w) => w["win"] === "win-e");
            assert.equal(newcomer?.["ws"], selectedLogical(admitted, "ws2"), "newcomer lands on the selected workspace");
            assert.deepEqual(admitted["switchSettled"], [], "terminal never replays on later captures");
            appliedMatchesReply(h, admitted);
            const at = h.wins.findIndex((w) => w.internalId === "win-d");
            assert.ok(at >= 0, "win-d present before close");
            const [removed] = h.wins.splice(at, 1);
            h.fire("windowRemoved", removed);
            h.runTimers();
            await h.settle();
            const closed = lastDescribeReply(h);
            const closedIds = ((closed["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(closedIds, ["win-a", "win-b", "win-c", "win-e"], "published window set exact after close");
            assert.deepEqual(closed["switchSettled"], [], "terminal never replays after close");
            appliedMatchesReply(h, closed);
        } finally {
            await h.stop();
        }
    });

    it("selects an empty workspace then admits a window onto it", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { desktops: ["ws1", "ws2"] });
        try {
            await discover(h);
            await h.settle();
            h.handle.requestWorkspaceSelect(2);
            await awaitQueueCount(h, "select-numbered", 1);
            const filed = h.commandPayloads("select-numbered");
            assert.equal(filed.length, 1, "selection files exactly once");
            assert.deepEqual(
                filed[0]?.body["command"],
                { op: "select-numbered", out: null, index: 2 },
                "selection carries the neutral cursor-first numbered command",
            );
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.viewWrites, ["out->ws2"], "native current moves to the empty target exactly once");
            assert.equal(nativeCurrent(h), "ws2", "native current reads back the empty target");
            const takes = takeReadys(h);
            assert.equal(takes.length, 1, "selection settles its own take");
            assert.equal(takes[0]?.["globalCmd"], "1", "take rides sequence 1");
            assert.equal(takes[0]?.["result"], null, "take resolves null: queue/take is report-result delivery, not a switch receipt");
            const reply = lastDescribeReply(h);
            assert.equal(reply["result"], null, "actuator lane never carries the selection terminal");
            // Focusless completion (workspaces decision): no required focus
            // target, so the binding reports `unproven` and completes once
            // visibility obligations are satisfied, with `failed: []`. This
            // is the offline counterpart of the native managed-empty
            // `complete/unproven` terminal in the v4-phase2 selection
            // evidence.
            assert.deepEqual(
                reply["switchSettled"],
                [{ op: "op-1", outcome: "complete", focus: "unproven", failed: [], fallback: false }],
                "focusless binding settles one terminal",
            );
            assert.deepEqual(reply["effects"], [], "no focus effect is invented for an empty target");
            assert.deepEqual(h.focused, [], "no native focus actuation without a target");
            const ids = ((reply["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(ids, ["win-a", "win-b"], "published window set exact after empty selection");
            appliedMatchesReply(h, reply);
            // Admit onto the selected empty workspace: placement follows the
            // selected cell per core output.
            const fresh = attachWindow(h, "win-c", { x: 600, y: 0, width: 200, height: 200 }, deskById(h, "ws2"));
            h.fire("windowAdded", fresh);
            h.runTimers();
            await h.settle();
            const admitted = lastDescribeReply(h);
            const admIds = ((admitted["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(admIds, ["win-a", "win-b", "win-c"], "published window set exact after admission");
            const newcomer = (admitted["windows"] as Array<Record<string, unknown>>).find((w) => w["win"] === "win-c");
            assert.equal(newcomer?.["ws"], selectedLogical(admitted, "ws2"), "newcomer lands on the selected workspace");
            appliedMatchesReply(h, admitted);
        } finally {
            await h.stop();
        }
    });

    it("supersedes a rapid return with no native view write and no focus effect", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { desktops: ["ws1", "ws2"] });
        try {
            await discover(h);
            await h.settle();
            // Both presses file before any capture dispatches (filing is
            // synchronous; the dispatch gate waits for every parked filing
            // to settle), so one capture drains both commands in order and
            // the second select supersedes the first open binding.
            h.handle.requestWorkspaceSelect(2);
            await awaitQueueCount(h, "select-numbered", 1);
            h.handle.requestWorkspaceSelect(1);
            await awaitQueueCount(h, "select-numbered", 2);
            assert.equal(h.commandPayloads("select-numbered").length, 2, "both presses file through the production route");
            await h.flush();
            h.runTimers();
            await h.settle();
            // Single-drain batch supersession (`retain_view_inner` replaces
            // the prior view flight per output): the second select prunes
            // the first open binding before any native attempt issues, so
            // with a net-no-op view no native write lands at all.
            assert.deepEqual(h.viewWrites, [], "superseded selection issues no native view write");
            assert.equal(nativeCurrent(h), "ws1", "native current never leaves the origin");
            const takes = takeReadys(h);
            assert.equal(takes.length, 2, "both commands settle their own takes");
            assert.deepEqual(takes.map((t) => t["globalCmd"]), ["1", "2"], "takes ride fresh sequences 1/2");
            assert.ok(takes.every((t) => t["result"] === null), "takes resolve null, never a switch receipt");
            const reply = lastDescribeReply(h);
            assert.equal(reply["result"], null, "actuator lane never carries a terminal");
            assert.deepEqual(reply["switchSettled"], [], "superseded open parent emits no terminal");
            assert.deepEqual(reply["effects"], [], "no arrival focus effect is filed");
            assert.deepEqual(h.focused, [], "no native focus actuation");
            appliedMatchesReply(h, reply);
        } finally {
            await h.stop();
        }
    });
});

describe("world genuine-producer fractional-scale journeys", () => {
    // Tier 3A: fractional current-scale geometry through the real entry,
    // retained broker and real core. The fake output carries
    // devicePixelRatio 1.25 with fractional logical window readbacks, as
    // real KWin reports them; logical screen/work bounds stay the default
    // integers (KWin logical space). Expectations derive from the contract
    // scale-and-rounding rule (floor(x+0.5): 0.4->0, 0.6->1, 0.5->1) and
    // the adapter ordinary-fact rule (world-observe.ts `toInt`:
    // per-component Math.round), never from parser output. Pump order per
    // leg is the established `discover` -> `settle`, event -> `runTimers`
    // -> `settle`; the normalized oracle replaces `appliedMatchesReply`.
    function envelopeReportAt(h: Harness, index: number): Record<string, unknown> {
        return describeEnvelopeAt(h, index)["report"] as Record<string, unknown>;
    }

    function reportRect(report: Record<string, unknown>, win: string): unknown {
        const rows = (report["windows"] as Array<Record<string, unknown>>) ?? [];
        return rows.find((w) => w["win"] === win)?.["rect"];
    }

    it("admits a newcomer at scale 1.25 with normalized fractional readbacks", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { fractionalScale: 1.25 });
        try {
            await discover(h);
            await h.settle();
            // Wire normalization on the first dispatched capture (nothing
            // actuates before the first reply, so it carries the seeds):
            // fractional natives ride normalized integers and wire scale
            // stays 1 despite the 1.25 DPR on the fake (contract
            // scale-and-rounding; scaled-metrics note).
            const first = h.calls.findIndex((call) => call.method === "DescribeWorld");
            assert.ok(first >= 0, "initial capture dispatched");
            const initial = envelopeReportAt(h, first);
            const screens = (initial["screens"] as Array<Record<string, unknown>>) ?? [];
            assert.equal(screens.length, 1, "single output rides");
            assert.equal((screens[0] as Record<string, unknown>)["scale"], 1, "wire scale stays 1 with a DPR 1.25 fake");
            assert.deepEqual(reportRect(initial, "win-a"), [0, 1, 200, 200], "fractional seed 0.4/0.6 normalizes per component");
            assert.deepEqual(reportRect(initial, "win-b"), [300, 1, 200, 200], "fractional seed 300.4/0.6 normalizes per component");
            const before = new Map(h.wins.map((w) => [w.internalId, { ...normalizedFrameOf(w) }]));
            // Newcomer arrives with a genuine fractional seed including
            // exact halves: 600.5 rounds toward +infinity to 601 and 20.5
            // to 21 (contract floor(x+0.5)), the normalization the live
            // 1.25 run never witnessed (scenario-runner note).
            const seed = { x: 600.5, y: 20.5, width: 200, height: 200 };
            const fresh = attachWindow(h, "win-c", seed);
            const eventAt = h.calls.length;
            h.fire("windowAdded", fresh);
            h.runTimers();
            await h.settle();
            // The first envelope carrying win-c predates any reply naming
            // win-c, so no actuation could have touched it: its rect is the
            // normalized seed, never the admission placement.
            const seedIndex = h.calls.findIndex((call, at) => {
                if (at < eventAt || call.method !== "DescribeWorld") {
                    return false;
                }
                try {
                    const report = (JSON.parse(call.payload) as Record<string, unknown>)["report"] as Record<string, unknown>;
                    const rows = (report["windows"] as Array<Record<string, unknown>>) ?? [];
                    return rows.some((w) => w["win"] === "win-c");
                } catch {
                    return false;
                }
            });
            assert.ok(seedIndex >= 0, "admission capture carries the newcomer seed");
            assert.deepEqual(reportRect(envelopeReportAt(h, seedIndex), "win-c"), [601, 21, 200, 200], "exact halves normalize toward +infinity");
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "admission capture accepted");
            const ids = ((reply["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(ids, ["win-a", "win-b", "win-c"], "published window set exact");
            appliedMatchesReplyNormalized(h, reply);
            const frames = replyFrames(reply);
            const newcomer = frames.get("win-c");
            assert.ok(newcomer !== undefined && newcomer.w > 0 && newcomer.h > 0, "newcomer carries a real tiled frame");
            assert.notDeepEqual(newcomer, { x: 601, y: 21, w: 200, h: 200 }, "admission replaces the normalized seed rect");
            assert.notDeepEqual(normalizedFrameOf(winById(h, "win-a")), before.get("win-a"), "focused neighbor reflows for the newcomer");
            assert.deepEqual(normalizedFrameOf(winById(h, "win-b")), before.get("win-b"), "far survivor keeps its exact frame");
        } finally {
            await h.stop();
        }
    });

    it("reflows survivors on close at scale 1.25 with normalized equality", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"], { fractionalScale: 1.25 });
        try {
            await discover(h);
            await h.settle();
            const before = new Map(h.wins.map((w) => [w.internalId, { ...normalizedFrameOf(w) }]));
            const at = h.wins.findIndex((w) => w.internalId === "win-c");
            assert.ok(at >= 0, "win-c present before close");
            const [removed] = h.wins.splice(at, 1);
            h.fire("windowRemoved", removed);
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "close capture accepted");
            const ids = ((reply["windows"] as Array<Record<string, unknown>>).map((w) => w["win"] as string)).sort();
            assert.deepEqual(ids, ["win-a", "win-b"], "published window set exact after close");
            appliedMatchesReplyNormalized(h, reply);
            const afterA = normalizedFrameOf(winById(h, "win-a"));
            assert.ok(areaOf(afterA) > areaOf(before.get("win-a") as { w: number; h: number }), "absorbing survivor strictly larger");
            assert.deepEqual(normalizedFrameOf(winById(h, "win-b")), before.get("win-b"), "far survivor keeps its exact frame");
        } finally {
            await h.stop();
        }
    });

    it("no-churn guard: publishes identical allocations with and without the fractional-scale fake", async () => {
        // No-churn guard (scaled-metrics note): the DPR rides the fake output
        // only; the adapter emits wire scale 1 as a constant and never reads
        // devicePixelRatio, so this guards against a future DPR read changing
        // allocations, not current behaviour. Same logical bounds and unscaled
        // gaps publish identical integer allocations.
        const plain = await makeHarness("win-a", ["win-a", "win-b"]);
        let plainFrames: Map<string, { x: number; y: number; w: number; h: number }>;
        try {
            await discover(plain);
            await plain.settle();
            plainFrames = replyFrames(lastDescribeReply(plain));
        } finally {
            await plain.stop();
        }
        const h = await makeHarness("win-a", ["win-a", "win-b"], { fractionalScale: 1.25 });
        try {
            await discover(h);
            await h.settle();
            const reply = lastDescribeReply(h);
            const scaledFrames = replyFrames(reply);
            const sorted = (m: Map<string, { x: number; y: number; w: number; h: number }>): Array<[string, { x: number; y: number; w: number; h: number }]> =>
                [...m.entries()].sort((a, b) => (a[0] < b[0] ? -1 : 1));
            assert.deepEqual(sorted(scaledFrames), sorted(plainFrames), "DPR-ignored logical route publishes identical allocations");
            appliedMatchesReplyNormalized(h, reply);
        } finally {
            await h.stop();
        }
    });

    it("normalized oracle fails on a one-unit post-rounding drift", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { fractionalScale: 1.25 });
        try {
            await discover(h);
            await h.settle();
            const reply = lastDescribeReply(h);
            appliedMatchesReplyNormalized(h, reply);
            // Drift one native component by exactly one post-rounding unit
            // (integers round to themselves): the oracle must fail loudly,
            // proving the journey assertions above are not vacuous.
            const win = winById(h, "win-a");
            const live = frameOf(win);
            win.frameGeometry = { x: live.x + 1, y: live.y, width: live.w, height: live.h };
            assert.throws(() => appliedMatchesReplyNormalized(h, reply), /native frame of/, "one-unit post-rounding drift fails the oracle");
            // A sub-half fractional wobble still normalizes back exactly:
            // rounding, never tolerance.
            const want = replyFrames(reply).get("win-a");
            assert.ok(want !== undefined, "published frame for win-a");
            win.frameGeometry = { x: want.x + 0.4, y: want.y, width: want.w, height: want.h };
            appliedMatchesReplyNormalized(h, reply);
        } finally {
            await h.stop();
        }
    });
});
