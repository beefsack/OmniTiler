// World actuation identity and proof (D3): the production entry drives
// every scenario below across multiple full report flights with real native
// call counts. Membership/view feedback uses the stable core transfer
// operation id (`op-<cmdSeq>`) exactly, never an invented `send-<win>` or
// `view-<win>` token; possibly-executed attempts never repeat without fresh
// unambiguous proof; Applied settles only on read-back proof; throws and
// partial writes stay ambiguous; proven external input supersedes; creation
// recovers by enumeration against the pre-attempt baseline; raise/stack
// never focus.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isWorldRejection } from "../src/world-report";

interface FakeDesktop {
    id: string;
    name: string;
}
interface FakeWindow {
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    fullscreen: boolean;
    maximized: boolean;
    sticky: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    minSize: null;
    maxSize: null;
}
interface Faults {
    deferMembership: boolean;
    throwMembership: "before" | "after" | null;
    createFaults: Record<string, "noop" | "throw-clean" | "throw-mutating">;
    signalOnCreate: boolean;
    geometryEchoSignal: boolean;
}
interface FakeWorld {
    screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>;
    desktops: FakeDesktop[];
    wins: FakeWindow[];
    activeScreen: { name: string };
    activeWindow: FakeWindow | null;
    currentDesktop: FakeDesktop;
    created: string[];
    createCalls: number;
    createArgs: unknown[][];
    removed: string[];
    focused: string[];
    viewSwitches: string[];
    membershipCalls: string[];
    membershipWrites: string[];
    geometryWrites: string[];
    outputWrites: string[];
}
interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}
interface FakeSignal {
    readonly handlers: Array<(payload?: unknown) => void>;
    readonly signal: { connect: (handler: (payload?: unknown) => void) => void; disconnect: (handler: (payload?: unknown) => void) => void };
}
interface Harness {
    world: FakeWorld;
    faults: Faults;
    pendingMembership: Map<string, FakeDesktop[]>;
    rawApplyMembership: Map<string, (refs: FakeDesktop[]) => void>;
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    signals: Record<string, FakeSignal>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: (maxDelay?: number) => void;
    fire: (name: string, payload?: unknown) => void;
    flushMembership: () => void;
}

function fakeSignal(): FakeSignal {
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
}

const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });
const win = (id: string, out: string, ws: FakeDesktop, x = 0): FakeWindow => ({
    internalId: id,
    normalWindow: true,
    frameGeometry: { x, y: 0, width: 200, height: 200 },
    output: { name: out },
    desktops: [ws],
    minimized: false,
    fullscreen: false,
    maximized: false,
    sticky: false,
    keepAbove: false,
    keepBelow: false,
    minSize: null,
    maxSize: null,
});

function makeHarness(mode: unknown = "per-output-local"): Harness {
    const faults: Faults = { deferMembership: false, throwMembership: null, createFaults: {}, signalOnCreate: false, geometryEchoSignal: false };
    const pendingMembership = new Map<string, FakeDesktop[]>();
    const rawApplyMembership = new Map<string, (refs: FakeDesktop[]) => void>();
    const ws1 = desk("ws1", "WS1");
    const ws2 = desk("ws2", "WS2");
    const ws3 = desk("ws3", "WS3");
    const w1 = win("win-a", "out", ws1, 0);
    const w2 = win("win-b", "out", ws2, 0);
    const world: FakeWorld = {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1, ws2, ws3],
        wins: [w1, w2],
        activeScreen: { name: "out" },
        activeWindow: w1,
        currentDesktop: ws1,
        created: [],
        createCalls: 0,
        createArgs: [],
        removed: [],
        focused: [],
        viewSwitches: [],
        membershipCalls: [],
        membershipWrites: [],
        geometryWrites: [],
        outputWrites: [],
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const signals: Record<string, FakeSignal> = {
        windowAdded: fakeSignal(),
        windowRemoved: fakeSignal(),
        windowActivated: fakeSignal(),
        desktopsChanged: fakeSignal(),
        screensChanged: fakeSignal(),
        currentDesktopChanged: fakeSignal(),
        activeScreenChanged: fakeSignal(),
    };
    const fire = (name: string, payload?: unknown): void => {
        for (const handler of [...(signals[name] as FakeSignal).handlers]) {
            handler(payload);
        }
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals.windowAdded as FakeSignal).signal,
        windowRemoved: (signals.windowRemoved as FakeSignal).signal,
        windowActivated: (signals.windowActivated as FakeSignal).signal,
        desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
        screensChanged: (signals.screensChanged as FakeSignal).signal,
        currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
        activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            const found = world.wins.find((w) => (w as unknown) === ref);
            if (found) {
                world.activeWindow = found;
                world.focused.push(found.internalId);
            }
        },
        get currentDesktop() {
            return world.currentDesktop;
        },
        set currentDesktop(ref: unknown) {
            const found = world.desktops.find((d) => (d as unknown) === ref);
            if (found) {
                world.currentDesktop = found;
            }
        },
        clientArea: (_area: number, _out: unknown, _desk: unknown) => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (_screen: unknown) => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown, _screen: unknown) => {
            const found = world.desktops.find((d) => (d as unknown) === deskRef);
            if (found) {
                world.currentDesktop = found;
                world.viewSwitches.push(found.id);
            }
        },
        sendClientToScreen: (mover: unknown, out: unknown) => {
            const w = world.wins.find((cand) => (cand as unknown) === mover);
            const name = (out as { name?: unknown }).name;
            if (w && typeof name === "string") {
                w.output = { name };
                world.outputWrites.push(`${w.internalId}=>${name}`);
            }
        },
        createDesktop: (...args: ReadonlyArray<unknown>) => {
            world.createCalls += 1;
            world.createArgs.push([...args]);
            // Producer-shaped KWin 6.7: required zero-based (position, name),
            // void return. Wrong-arity calls throw Insufficient arguments,
            // as live. The logical token rides as the native name.
            if (args.length !== 2 || typeof args[0] !== "number" || typeof args[1] !== "string" || (args[1] as string) === "") {
                throw new Error("Insufficient arguments");
            }
            const name = args[1] as string;
            const fault = faults.createFaults[name] ?? "ok";
            if (fault === "throw-clean") {
                throw new Error("create-boom-clean");
            }
            if (fault === "noop") {
                return;
            }
            const id = `native-${world.desktops.length + 1}`;
            world.desktops.push(desk(id, name));
            world.created.push(id);
            if (fault === "throw-mutating") {
                throw new Error("create-boom-after-side-effect");
            }
            if (faults.signalOnCreate) {
                fire("desktopsChanged");
            }
        },
        removeDesktop: (ref: unknown) => {
            const id = (ref as FakeDesktop).id;
            world.desktops = world.desktops.filter((d) => d.id !== id);
            world.removed.push(id);
        },
    };
    for (const w of world.wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                if (faults.geometryEchoSignal) {
                    fire("windowActivated");
                }
                return true;
            },
        });
        let desks = w.desktops;
        const applyDesks = (refs: FakeDesktop[]): void => {
            desks = refs;
            world.membershipWrites.push(`${w.internalId}->${refs.map((d) => d.id).join("+")}`);
        };
        rawApplyMembership.set(w.internalId, applyDesks);
        Object.defineProperty(w, "desktops", {
            configurable: true,
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                world.membershipCalls.push(`${w.internalId}->${refs.map((d) => d.id).join("+")}`);
                if (faults.deferMembership) {
                    pendingMembership.set(w.internalId, refs);
                    return true;
                }
                if (faults.throwMembership === "before") {
                    throw new Error("membership-boom-before");
                }
                applyDesks(refs);
                if (faults.throwMembership === "after") {
                    throw new Error("membership-boom-after");
                }
                return true;
            },
        });
    }
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (message) => {
            logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => mode,
    });
    assert.ok(handle !== null);
    const runTimers = (maxDelay = 120): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
    };
    const flushMembership = (): void => {
        for (const [id, refs] of [...pendingMembership.entries()]) {
            pendingMembership.delete(id);
            // Late native completion outside any adapter setter: no new
            // adapter attempt is recorded.
            rawApplyMembership.get(id)?.(refs);
            world.membershipWrites.push(`${id}->${refs.map((d) => d.id).join("+")} (flushed)`);
        }
    };
    return {
        world, faults, pendingMembership, rawApplyMembership, calls, timers, logs, signals, handle,
        runTimers, fire, flushMembership,
    };
}

const BOOTSTRAP = (gen: string, req: string): string => JSON.stringify({
    v: 1, adapter: "test-adapter", gen, req, obs: "0", rev: "0",
    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
    windows: [], transfers: [], effects: [],
});

function discover(h: Harness, gen = "gen-1"): void {
    const call = h.calls.filter((c) => c.method === "DiscoverWorld").pop() as CapturedCall;
    const body = JSON.parse(call.payload);
    call.callback(BOOTSTRAP(gen, body.req));
    const auto = h.calls.filter((c) => c.method === "DescribeWorld").pop() as CapturedCall;
    const autoBody = JSON.parse(auto.payload);
    assert.equal(autoBody.command, null);
    auto.callback(acceptedReply(gen, autoBody.req, autoBody.obs, "0"));
}

function acceptedReply(gen: string, req: string, obs: string, rev: string, extra: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 1, adapter: "test-adapter", gen, req, obs, rev,
        ready: true, consumed: false,
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        ...extra,
    });
}

function lastDescribe(h: Harness): CapturedCall {
    const found = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(found !== undefined, "expected DescribeWorld");
    return found as CapturedCall;
}

// Dispatch one debounced report (timers) and reply accepted with rev from
// the counter. Returns the request payload the reply answered.
function dispatchAndReply(h: Harness, rev: { n: number }, extra: Record<string, unknown> = {}): Record<string, unknown> {
    h.runTimers();
    const call = lastDescribe(h);
    const body = JSON.parse(call.payload) as Record<string, unknown>;
    rev.n += 1;
    call.callback(acceptedReply("gen-1", body["req"] as string, body["obs"] as string, String(rev.n), extra));
    return body;
}

// Answer any pending feedback-delivery flight with a quiet accept so
// the next leg starts idle: a reply to an already-settled flight is
// ignored by its settled guard, so this is safe either way.
function drain(h: Harness, rev: { n: number }): void {
    dispatchAndReply(h, rev, {});
    h.runTimers();
}

function feedbackOf(h: Harness): Array<Record<string, unknown>> {
    return (JSON.parse(lastDescribe(h).payload) as Record<string, unknown>)["feedback"] as Array<Record<string, unknown>>;
}

function noInventedOps(rows: Array<Record<string, unknown>>): void {
    for (const row of rows) {
        const op = row["op"] as string;
        assert.ok(!op.startsWith("send-"), `invented membership op ${op}`);
        assert.ok(!op.startsWith("view-"), `invented view op ${op}`);
        assert.ok(!op.startsWith("prop-") || row["kind"] === "property", `prop op on ${row["kind"]}`);
    }
}

const winRow = (winId: string, ws: string): Record<string, unknown> => ({
    win: winId, out: "out", ws,
    alloc: null, frame: null,
    frame_auth: "controlled", frame_hold: "",
    member_auth: "controlled", member_hold: "",
    frame_op: "",
    floating: false, float_origin: "",
});

// Floating-workspace boundary row: membership-only at the target homing,
// frame untouched (never geometry on floats).
const boundaryRow = (winId: string, ws: string): Record<string, unknown> => ({
    win: winId, out: "out", ws,
    alloc: null, frame: null,
    frame_auth: "untouched", frame_hold: "",
    member_auth: "controlled", member_hold: "",
    frame_op: "",
    floating: false, float_origin: "",
});

// Explicit untouched floating lifetime: authorizes nothing.
const untouchedRow = (winId: string, ws: string): Record<string, unknown> => ({
    win: winId, out: "out", ws,
    alloc: null, frame: null,
    frame_auth: "untouched", frame_hold: "",
    member_auth: "untouched", member_hold: "",
    frame_op: "",
    floating: false, float_origin: "",
});

describe("world actuation identity and proof", () => {
    it("moves under the stable transfer op and never repeats while ambiguous", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "one native membership attempt");
        // The native side applied synchronously, so read-back proof settles
        // applied on the same flight under the stable core op.
        h.runTimers();
        let feedback = feedbackOf(h);
        noInventedOps(feedback);
        const pending = feedback.find((f) => f["op"] === "op-1");
        assert.ok(pending !== undefined, "stable op-1 feedback rides");
        assert.equal(pending["kind"], "transfer");
        assert.equal(pending["outcome"], "applied");
        // The native side has converged meanwhile: the next flight confirms
        // with read-back proof and issues no second setter call.
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "confirmed without repeating the setter");
        h.runTimers();
        feedback = feedbackOf(h);
        const settled = feedback.find((f) => f["op"] === "op-1");
        assert.ok(settled !== undefined);
        assert.equal(settled["outcome"], "applied");
        // A repeated identical desired target stays quiet.
        dispatchAndReply(h, rev, { windows: [winRow("win-a", "ws2")] });
        assert.equal(h.world.membershipCalls.length, 1, "identical target never re-actuates");
    });

    it("actuates boundary membership into floating scopes with no float geometry", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelativeMove(1, true);
        // Core boundary reply: floating scope untouched with explicit
        // untouched rows, plus one membership-only row at the floating
        // target under the stable transfer op.
        dispatchAndReply(h, rev, {
            scopes: [
                { scope: "out/ws1", auth: "controlled", reason: "" },
                { scope: "out/ws2", auth: "untouched", reason: "" },
            ],
            windows: [boundaryRow("win-a", "ws2"), untouchedRow("win-b", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "boundary membership actuates natively");
        assert.equal(h.world.geometryWrites.length, 0, "no frame writes on the floating side");
        h.runTimers();
        const feedback = feedbackOf(h);
        const pending = feedback.find((f) => f["op"] === "op-1");
        assert.ok(pending !== undefined, "stable op-1 feedback rides");
        assert.equal(pending["outcome"], "applied");
        // Untouched rows never actuate on their own: a reply without the
        // transfer stays completely quiet.
        dispatchAndReply(h, rev, {
            scopes: [
                { scope: "out/ws1", auth: "controlled", reason: "" },
                { scope: "out/ws2", auth: "untouched", reason: "" },
            ],
            windows: [untouchedRow("win-a", "ws2"), untouchedRow("win-b", "ws2")],
        });
        assert.equal(h.world.membershipCalls.length, 1, "untouched rows authorize nothing");
        assert.equal(h.world.geometryWrites.length, 0, "untouched rows write no frames");
    });

    it("actuates both-floating sends membership-only like the boundary", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelativeMove(1, true);
        // Both scopes untouched with explicit rows, plus one
        // membership-only row at the target under the stable transfer.
        dispatchAndReply(h, rev, {
            scopes: [
                { scope: "out/ws1", auth: "untouched", reason: "" },
                { scope: "out/ws2", auth: "untouched", reason: "" },
            ],
            windows: [boundaryRow("win-a", "ws2"), untouchedRow("win-b", "ws2")],
            transfers: [{ op: "op-9", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: true, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "both-floating membership actuates natively");
        assert.equal(h.world.geometryWrites.length, 0, "both-floating writes no frames");
        h.runTimers();
        const feedback = feedbackOf(h);
        const settled = feedback.find((f) => f["op"] === "op-9");
        assert.ok(settled !== undefined);
        assert.equal(settled["outcome"], "applied");
    });

    it("holds delayed native completion without repeating, then confirms", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.faults.deferMembership = true;
        h.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1);
        // Live still shows the source: the retry flight must not re-attempt.
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws1")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: true }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "ambiguous op never repeats");
        h.runTimers();
        const held = feedbackOf(h).find((f) => f["op"] === "op-1");
        assert.ok(held !== undefined);
        assert.equal(held["outcome"], "possibly-executed");
        // The native side completes late: flush plus a native event, then
        // the next flight confirms with proof and no new setter call.
        h.faults.deferMembership = false;
        h.flushMembership();
        h.fire("windowActivated");
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: true }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "late proof confirms without a second attempt");
        h.runTimers();
        const settled = feedbackOf(h).find((f) => f["op"] === "op-1");
        assert.ok(settled !== undefined);
        assert.equal(settled["outcome"], "applied");
    });

    it("treats a throwing setter as ambiguous and a mutating throw as applied", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.faults.throwMembership = "before";
        h.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1);
        h.runTimers();
        const ambiguous = feedbackOf(h).find((f) => f["op"] === "op-1");
        assert.ok(ambiguous !== undefined);
        assert.equal(ambiguous["outcome"], "possibly-executed", "throw stays ambiguous, never refused-proven");
        // No repeat while ambiguous.
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws1")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: true }],
        });
        assert.equal(h.world.membershipCalls.length, 1);
        // The fault clears but nothing converged natively: the ambiguous
        // operation still never repeats without fresh proof.
        h.faults.throwMembership = null;
        h.fire("windowActivated");
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws1")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: true }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "ambiguous op never repeats");
        h.runTimers();
        assert.equal(feedbackOf(h).find((f) => f["op"] === "op-1")?.["outcome"], "possibly-executed");
        // Late native convergence completes it with proof, no new attempt.
        h.rawApplyMembership.get("win-a")?.([h.world.desktops.find((d) => d.id === "ws2") as FakeDesktop]);
        h.fire("windowActivated");
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: true }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "proof confirms without a second attempt");
        h.runTimers();
        const settled = feedbackOf(h).find((f) => f["op"] === "op-1");
        assert.ok(settled !== undefined);
        assert.equal(settled["outcome"], "applied");
        // A mutating throw still proves applied on read-back.
        const h2 = makeHarness();
        discover(h2);
        const rev2 = { n: 0 };
        h2.faults.throwMembership = "after";
        h2.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h2, rev2, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        h2.runTimers();
        const proven = feedbackOf(h2).find((f) => f["op"] === "op-1");
        assert.ok(proven !== undefined);
        assert.equal(proven["outcome"], "applied", "read-back proof wins over the throw");
    });

    it("actuates a view-only transfer under its stable op with no repeat", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const viewTransfer = (held: boolean): Record<string, unknown> => ({
            op: "op-1", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held,
        });
        dispatchAndReply(h, rev, {
            outputs: [{ out: "out", view: "ws2", mode: "tiled", hold: "" }],
            transfers: [viewTransfer(false)],
        });
        assert.deepEqual(h.world.viewSwitches, ["ws2"], "one native view switch");
        h.runTimers();
        let feedback = feedbackOf(h);
        noInventedOps(feedback);
        const pending = feedback.find((f) => f["op"] === "op-1");
        assert.ok(pending !== undefined);
        assert.equal(pending["kind"], "view");
        assert.equal(pending["outcome"], "applied", "synchronous proof settles on the same flight");
        // Identical view target confirms without a second switch.
        dispatchAndReply(h, rev, {
            outputs: [{ out: "out", view: "ws2", mode: "tiled", hold: "" }],
            transfers: [viewTransfer(false)],
        });
        assert.deepEqual(h.world.viewSwitches, ["ws2"], "confirmed view never re-switches");
        h.runTimers();
        feedback = feedbackOf(h);
        assert.equal(feedback.find((f) => f["op"] === "op-1")?.["outcome"], "applied");
    });

    it("follows a window transfer view under the same operation", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: true, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1);
        assert.deepEqual(h.world.viewSwitches, ["ws2"], "follow switches under the same op");
        h.runTimers();
        const feedback = feedbackOf(h);
        noInventedOps(feedback);
        assert.ok(!feedback.some((f) => (f["op"] as string).startsWith("view-")), "no synthetic follow identity");
        assert.equal(feedback.find((f) => f["op"] === "op-1")?.["outcome"], "applied");
    });

    it("moves across outputs with freshness between send and desktop set", () => {
        const h = makeHarness();
        h.world.screens.push({ name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } });
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        dispatchAndReply(h, rev, {
            windows: [{ ...winRow("win-a", "ws2"), out: "out-2" }],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out-2", ws: "ws2", follow: false, held: false }],
        });
        assert.deepEqual(h.world.outputWrites, ["win-a=>out-2"], "output send ran first");
        assert.equal(h.world.membershipCalls.length, 1, "desktop set ran second");
        h.runTimers();
        const feedback = feedbackOf(h);
        noInventedOps(feedback);
        assert.equal(feedback.find((f) => f["op"] === "op-1")?.["outcome"], "applied", "both steps proved on read-back");
        // No repeat once homed across outputs.
        dispatchAndReply(h, rev, {
            windows: [{ ...winRow("win-a", "ws2"), out: "out-2" }],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out-2", ws: "ws2", follow: false, held: false }],
        });
        assert.deepEqual(h.world.outputWrites, ["win-a=>out-2"], "cross-output target never re-actuates");
    });

    it("allows a fresh newer send for the same window after applied", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "confirmation issues no new attempt");
        // A distinct newer operation back to ws1 is a new actuation.
        h.handle?.requestWorkspaceRelativeMove(-1, true);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws1")],
            transfers: [{ op: "op-2", kind: "window", win: "win-a", out: "out", ws: "ws1", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 2, "fresh distinct op actuates");
    });

    it("supersedes on proven external membership input without fighting", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1);
        // The user drags win-a to a third workspace outside the operation
        // (external native input, not an adapter attempt).
        const ws3 = h.world.desktops.find((d) => d.id === "ws3") as FakeDesktop;
        h.rawApplyMembership.get("win-a")?.([ws3]);
        h.fire("desktopsChanged");
        const callsBefore = h.world.membershipCalls.length;
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, callsBefore, "external input is never fought");
        h.runTimers();
        const feedback = feedbackOf(h);
        assert.equal(feedback.find((f) => f["op"] === "op-1")?.["outcome"], "superseded-external");
    });

    it("applies self-feedback multi-window work but stops on native input", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        // Two retained sends actuate in one flight despite arming dirty.
        h.handle?.requestWorkspaceRelativeMove(1, true);
        dispatchAndReply(h, rev, {});
        h.handle?.requestWorkspaceRelativeMove(-1, true);
        h.runTimers();
        const callsBefore = h.world.membershipCalls.length;
        const both = lastDescribe(h);
        const body = JSON.parse(both.payload) as Record<string, unknown>;
        rev.n += 1;
        both.callback(acceptedReply("gen-1", body["req"] as string, body["obs"] as string, String(rev.n), {
            windows: [winRow("win-a", "ws2"), winRow("win-b", "ws1")],
            transfers: [
                { op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false },
                { op: "op-2", kind: "window", win: "win-b", out: "out", ws: "ws1", follow: false, held: false },
            ],
        }));
        assert.equal(h.world.membershipCalls.length, callsBefore + 2, "self feedback never stops sibling work");
        // A synchronous native echo between setters stops the remainder.
        const h2 = makeHarness();
        discover(h2);
        const rev2 = { n: 0 };
        h2.faults.geometryEchoSignal = true;
        h2.handle?.requestWorkspaceRelative(1);
        const geom = (id: string, x: number): Record<string, unknown> => ({
            win: id, out: "out", ws: id === "win-a" ? "ws1" : "ws2",
            alloc: [x, x, 300, 300], frame: [x, x, 300, 300],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            frame_op: "",
            floating: false, float_origin: "",
        });
        dispatchAndReply(h2, rev2, { windows: [geom("win-a", 11), geom("win-b", 22)] });
        assert.ok(h2.world.geometryWrites.some((w) => w.startsWith("win-a@11,11")), "first geometry applies");
        assert.ok(!h2.world.geometryWrites.some((w) => w.startsWith("win-b@22,22")), "input invalidation stops the rest");
    });

    it("creates with zero-based position/name and proves by enumeration", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const unbound = {
            id: "wl-1", out: "out", bind: "", logical: "wl-1", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "one creation flight");
        assert.deepEqual(h.world.createArgs[0], [3, "wl-1"], "explicit zero-based position plus logical name");
        assert.equal(h.world.created.length, 1, "void native created, enumeration proves");
        // Identical unbound rows never repeat: enumeration already bound.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "bound create never repeats");
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "still exactly one native create");
    });

    it("holds a no-op void create ambiguous without repeating", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "noop";
        const unbound = {
            id: "wl-1", out: "out", bind: "", logical: "wl-1", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1);
        assert.equal(h.world.created.length, 0, "no-op created nothing");
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "ambiguous no-op never repeats blindly");
    });

    it("recovers a mutating throw by name and holds a clean throw", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "throw-mutating";
        const unbound = {
            id: "wl-1", out: "out", bind: "", logical: "wl-1", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1);
        assert.equal(h.world.created.length, 1, "throw happened after the side-effect");
        delete h.faults.createFaults["wl-1"];
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "throw+side-effect proves, never repeats");
        const h2 = makeHarness();
        discover(h2);
        const rev2 = { n: 0 };
        h2.handle?.requestWorkspaceRelative(1);
        h2.faults.createFaults["wl-1"] = "throw-clean";
        dispatchAndReply(h2, rev2, { workspaces: [unbound] });
        assert.equal(h2.world.createCalls, 1);
        assert.equal(h2.world.created.length, 0, "clean throw created nothing");
        delete h2.faults.createFaults["wl-1"];
        h2.fire("desktopsChanged");
        dispatchAndReply(h2, rev2, { workspaces: [unbound] });
        assert.equal(h2.world.createCalls, 1, "clean throw stays ambiguous, never repeats blindly");
    });

    it("never shares one native desktop between two tokens in the same reply", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        // Second token clean-throws: it must not adopt the first token's desktop.
        h.faults.createFaults["wl-b"] = "throw-clean";
        const unboundA = {
            id: "wl-a", out: "out", bind: "", logical: "wl-a", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        const unboundB = {
            id: "wl-b", out: "out", bind: "", logical: "wl-b", scope: "out", order: 4, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB] });
        assert.equal(h.world.createCalls, 2, "each token attempts once from its own baseline");
        assert.deepEqual(h.world.createArgs[0], [3, "wl-a"], "first token appends at count");
        assert.deepEqual(h.world.createArgs[1], [4, "wl-b"], "second token baselines past the first creation");
        assert.equal(h.world.created.length, 1, "only the first token created");
        assert.ok(h.logs.some((l) => l.includes(" mk=1 ")), "only the proven token counts, no steal");
        // The clean-throwing token stays ambiguous: possibly-executed rides,
        // never the sibling's applied proof.
        h.runTimers();
        const fb = feedbackOf(h);
        assert.equal(fb.find((f) => f["op"] === "create-wl-a")?.["outcome"], "applied");
        assert.equal(fb.find((f) => f["op"] === "create-wl-b")?.["outcome"], "possibly-executed", "no steal of the sibling desktop");
        // Next flight: the bound token never repeats and the ambiguous
        // token never repeats blindly either.
        delete h.faults.createFaults["wl-b"];
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB] });
        assert.equal(h.world.createCalls, 2, "ambiguous token never repeats, bound token never repeats");
        const names = h.world.desktops.map((d) => d.name);
        assert.equal(names.filter((n) => n === "wl-a").length, 1);
        assert.ok(!names.includes("wl-b"), "no fabricated binding");
    });

    it("binds two successful creates in the same reply independently", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const unboundA = {
            id: "wl-a", out: "out", bind: "", logical: "wl-a", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        const unboundB = {
            id: "wl-b", out: "out", bind: "", logical: "wl-b", scope: "out", order: 4, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB] });
        assert.equal(h.world.createCalls, 2);
        assert.equal(h.world.created.length, 2, "two natives created");
        assert.ok(h.logs.some((l) => l.includes(" mk=2 ")), "both creations prove same-flight");
        const names = h.world.desktops.map((d) => d.name);
        assert.ok(names.includes("wl-a") && names.includes("wl-b"), "each native carries its token name");
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB] });
        assert.equal(h.world.createCalls, 2, "no perpetual ambiguity, no repeats");
    });

    it("never adopts an unrelated user-appended desktop while ambiguous", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "noop";
        const unbound = {
            id: "wl-1", out: "out", bind: "", logical: "wl-1", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1);
        // The user appends an unrelated desktop natively (different name).
        h.world.desktops.push(desk("user-9", "User 9"));
        delete h.faults.createFaults["wl-1"];
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "unrelated desktop never binds, no blind repeat");
        h.runTimers();
        const fb = feedbackOf(h);
        assert.equal(fb.find((f) => f["op"] === "create-wl-1")?.["outcome"], "possibly-executed", "stays ambiguous, never proven by a stranger");
        // Even healthy, the still-ambiguous token never re-attempts.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "still held, still no repeat");
        assert.ok(!h.world.desktops.map((d) => d.name).includes("wl-1"), "no fabricated binding");
    });

    it("holds an unreadable baseline unattempted and proceeds once readable", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const unbound = {
            id: "wl-1", out: "out", bind: "", logical: "wl-1", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        // Corrupt one native name read: the named snapshot is incomplete,
        // while id-only observation capture still succeeds.
        const ws1 = h.world.desktops[0] as unknown as Record<string, unknown>;
        const saved = ws1["name"];
        ws1["name"] = 42;
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 0, "incomplete inventory attempts nothing");
        assert.equal(h.world.created.length, 0);
        h.runTimers();
        assert.equal(feedbackOf(h).find((f) => f["op"] === "create-wl-1")?.["outcome"], "unattempted", "read-only failure never claims a native attempt");
        drain(h, rev);
        ws1["name"] = saved;
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1, "fresh readable baseline permits the first native attempt");
        assert.equal(h.world.desktops.filter((d) => d.name === "wl-1").length, 1);
    });

    it("settles a named delayed creation exactly once without another native call", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "noop";
        const unbound = {
            id: "wl-1", out: "out", bind: "", logical: "wl-1", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        h.world.desktops.push(desk("delayed-1", "wl-1"));
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1);
        assert.equal(h.logs.filter((l) => l.includes(" mk=1 ")).length, 1);
        h.runTimers();
        assert.equal(feedbackOf(h).find((f) => f["op"] === "create-wl-1")?.["outcome"], "applied");
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 1);
        assert.equal(h.logs.filter((l) => l.includes(" mk=1 ")).length, 1);
    });

    it("survives a synchronous desktop signal during creation without duplication", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.signalOnCreate = true;
        const unboundA = {
            id: "wl-a", out: "out", bind: "", logical: "wl-a", scope: "out", order: 3, mode: "tiled", exist: true, hold: "",
        };
        const unboundB = {
            id: "wl-b", out: "out", bind: "", logical: "wl-b", scope: "out", order: 4, mode: "tiled", exist: true, hold: "",
        };
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB] });
        assert.ok(h.world.createCalls >= 1, "creation attempted despite reentrant invalidation");
        assert.ok(h.world.createCalls <= 2, "reentrancy never duplicates within one flight");
        h.faults.signalOnCreate = false;
        h.fire("desktopsChanged");
        const before = h.world.createCalls;
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB] });
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB] });
        assert.ok(h.world.createCalls <= before + 2, "recovery converges without duplication");
        const boundNames = h.world.desktops.map((d) => d.name);
        const aCount = boundNames.filter((n) => n === "wl-a").length;
        const bCount = boundNames.filter((n) => n === "wl-b").length;
        assert.ok(aCount <= 1 && bCount <= 1, "no token created twice");
    });

    it("removes only provably empty workspaces and keeps referenced ones", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const absent = (id: string): Record<string, unknown> => ({
            id, out: "out", bind: id, logical: "", scope: "out", order: 2, mode: "tiled", exist: false, hold: "",
        });
        dispatchAndReply(h, rev, {
            windows: [winRow("win-b", "ws2")],
            workspaces: [absent("ws3"), absent("ws2")],
        });
        assert.ok(h.world.removed.includes("ws3"), "provably empty workspace removed");
        assert.ok(!h.world.removed.includes("ws2"), "window-referenced workspace retained");
    });

    it("never focuses for raise/stack but applies focus with proof", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        dispatchAndReply(h, rev, {
            effects: [
                { id: "fx-9", kind: "raise", target: "win-b", rev: "1" },
                { id: "fx-10", kind: "stack", target: "win-b", rev: "1" },
            ],
        });
        assert.equal(h.world.focused.length, 0, "raise/stack never steer focus");
        h.runTimers();
        let feedback = feedbackOf(h);
        assert.equal(feedback.find((f) => f["op"] === "fx-9")?.["outcome"], "unattempted");
        assert.equal(feedback.find((f) => f["op"] === "fx-10")?.["outcome"], "unattempted");
        h.handle?.requestWorkspaceRelative(-1);
        dispatchAndReply(h, rev, {
            effects: [{ id: "fx-11", kind: "focus", target: "win-b", rev: "2" }],
        });
        assert.deepEqual(h.world.focused, ["win-b"], "focus applies once with proof");
        h.runTimers();
        feedback = feedbackOf(h);
        assert.equal(feedback.find((f) => f["op"] === "fx-11")?.["outcome"], "applied");
    });
});

describe("world apply safety reads", () => {
    function directEnv(overrides: Partial<ApplyEnv> & { state: WorldAdapterState; observation: ApplyEnv["observation"]; reply: ApplyEnv["reply"] }): ApplyEnv {
        return {
            log: () => undefined,
            isStillListed: () => true,
            isActiveWindow: () => false,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws1" }),
            readLiveSafety: () => null,
            setGeometry: () => { throw new Error("must not write on unreadable safety"); },
            setMembership: () => { throw new Error("must not write on unreadable safety"); },
            sendToOutput: () => { throw new Error("must not write on unreadable safety"); },
            resolveOutput: () => null,
            resolveDesktop: () => null,
            switchView: () => { throw new Error("must not write on unreadable safety"); },
            createDesktop: () => null,
            removeDesktop: () => false,
            focusWindow: () => { throw new Error("must not write on unreadable safety"); },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => null,
            desktopById: () => null,
            minDesktops: 2,
            ...overrides,
        };
    }

    it("holds narrowly on unreadable safety instead of writing", () => {
        const state = new WorldAdapterState("test-adapter", "owner-a");
        state.onDiscovery("gen-1");
        state.invalidate();
        const begun = state.beginDispatch(null);
        assert.ok(begun !== null);
        const dispatchObs = state.currentObs;
        const text = JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: "req-1",
            obs: dispatchObs, rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                floating: false, float_origin: "",
            }],
            transfers: [], effects: [],
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && !isWorldRejection(parsed));
        if (isWorldRejection(parsed) || parsed === null) {
            return;
        }
        const verdict = state.acceptReply(parsed, dispatchObs);
        assert.deepEqual(verdict, { applied: true, reason: "accepted" });
        const ref = {};
        const observation: ApplyEnv["observation"] = {
            mode: "per-output-local",
            topology: [{ out: "out", order: ["ws1"], current: "ws1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
            bindings: [],
            domains: [{
                id: "out", ws: "ws1", bounds: { x: 0, y: 0, w: 800, h: 600 },
                gap: 8, outer: 0, focused: "", readable: true, hold: "",
                adjacent: {}, complete: true,
                windows: [{
                    win: "win-a", ref, out: "out", ws: "ws1",
                    rect: { x: 0, y: 0, w: 100, h: 100 },
                    floating: false, fitExcluded: false, fullscreen: false,
                    maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false,
                    hints: { minW: null, minH: null, maxW: null, maxH: null },
                    minimized: false,
                }],
                closed: [],
            }],
            minimized: [],
            predicate: "both-axes-fixed",
            complete: true,
            gesture: false,
            focusedGlobal: "",
        };
        const env = directEnv({ state, observation, reply: parsed });
        const result = applyWorldReply(env);
        assert.equal(result.geometryWrites, 0, "unreadable safety writes nothing");
        assert.equal(result.stoppedMidApply, true, "unreadable safety stops the apply");
        assert.ok(result.held >= 0);
    });
});

describe("world hotplug displacement production", () => {
    const wsRow = (id: string, out: string, order: number): Record<string, unknown> => ({
        id, out, bind: id, logical: "", scope: out, order, mode: "tiled", exist: true, hold: "",
    });
    const scopeRow = (scope: string, auth: string): Record<string, unknown> => ({ scope, auth, reason: "" });
    const outRow = (out: string, view: string): Record<string, unknown> => ({ out, view, mode: "tiled", hold: "" });
    // Membership-only row at the target homing, exactly as core publishes
    // for hotplug policy transfers (frame untouched, never geometry).
    const memberRow = (winId: string, out: string, ws: string): Record<string, unknown> => ({
        win: winId, out, ws, alloc: null, frame: null,
        frame_auth: "untouched", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "",
        floating: false, float_origin: "",
    });

    for (const mode of ["per-output-local", "global-unique", "shared"]) {
        it(`remove/re-add actuates natively in ${mode} without duplication`, () => {
            const h = makeHarness(mode);
            // Two-output native start: win-b lives on out-2/ws2.
            h.world.screens.push({ name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } });
            const winB = h.world.wins.find((w) => w.internalId === "win-b") as FakeWindow;
            winB.output = { name: "out-2" };
            h.world.activeWindow = winB;
            h.world.activeScreen = { name: "out-2" };
            discover(h);
            const rev = { n: 0 };
            const displaced = mode !== "shared";
            // Native unplug: out-2 gone, KWin rehomes the window output,
            // the desktop persists, focus stays native.
            h.world.screens = h.world.screens.filter((s) => s.name !== "out-2");
            winB.output = { name: "out" };
            h.world.activeScreen = { name: "out" };
            h.fire("screensChanged");
            const membershipBefore = h.world.membershipCalls.length;
            const viewsBefore = h.world.viewSwitches.length;
            if (displaced) {
                // Core displacement reply shape: survivor rows for both
                // workspaces, untouched floating-free scopes, one policy
                // view plus member transfers only for lagged members.
                // win-b already shows the survivor homing, so membership
                // confirms while the hidden view actuates.
                dispatchAndReply(h, rev, {
                    scopes: [scopeRow("out/ws1", "controlled"), scopeRow("out/ws2", "controlled")],
                    workspaces: [wsRow("ws1", "out", 0), wsRow("ws2", "out", 1)],
                    outputs: [outRow("out", "ws2")],
                    windows: [memberRow("win-b", "out", "ws2")],
                    transfers: [
                        { op: "op-disp-out-ws2", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false },
                    ],
                });
                assert.deepEqual(h.world.viewSwitches.slice(viewsBefore), ["ws2"], "displaced view actuates natively");
                assert.equal(h.world.membershipCalls.length, membershipBefore, "already-homed member issues no setter");
                assert.equal(h.world.geometryWrites.length, 0, "no float geometry anywhere");
                h.runTimers();
                const proved = feedbackOf(h).find((f) => f["op"] === "op-disp-out-ws2");
                assert.ok(proved !== undefined);
                assert.equal(proved["outcome"], "applied", "view proof settles on the same flight");
                drain(h, rev);
            } else {
                // Shared mode displaces nothing: the reply carries no
                // transfers and the adapter stays completely quiet.
                dispatchAndReply(h, rev, {
                    scopes: [scopeRow("out/ws1", "controlled"), scopeRow("out/ws2", "controlled")],
                    workspaces: [wsRow("ws1", "out", 0), wsRow("ws2", "out", 1)],
                    outputs: [outRow("out", "ws1")],
                    windows: [memberRow("win-b", "out", "ws2")],
                    transfers: [],
                });
                assert.equal(h.world.viewSwitches.length, viewsBefore, "shared switches nothing");
                assert.equal(h.world.membershipCalls.length, membershipBefore, "shared moves nothing");
                drain(h, rev);
            }
            // Native re-plug with lag: win-b still shows the survivor
            // homing while its workspace is home. A user-moved window
            // (win-a dragged to ws3 natively) is never pulled back: the
            // core reply below names no transfer for it.
            h.world.screens.push({ name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } });
            h.fire("screensChanged");
            const winA = h.world.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            const ws3 = h.world.desktops.find((d) => d.id === "ws3") as FakeDesktop;
            winA.desktops = [ws3];
            const memberBefore = h.world.membershipCalls.length;
            const outputBefore = h.world.outputWrites.length;
            const focusedBefore = [...h.world.focused];
            if (displaced) {
                dispatchAndReply(h, rev, {
                    scopes: [scopeRow("out/ws1", "controlled"), scopeRow("out-2/ws2", "controlled")],
                    workspaces: [wsRow("ws1", "out", 0), wsRow("ws2", "out-2", 0)],
                    outputs: [outRow("out", "ws1"), outRow("out-2", "ws2")],
                    windows: [memberRow("win-b", "out-2", "ws2")],
                    transfers: [
                        { op: "op-hp-win-b", kind: "window", win: "win-b", out: "out-2", ws: "ws2", follow: false, held: false },
                        { op: "op-disp-out-2-ws2", kind: "view", win: "", out: "out-2", ws: "ws2", follow: false, held: false },
                    ],
                });
                assert.ok(
                    h.world.outputWrites.slice(outputBefore).some((w) => w === "win-b=>out-2"),
                    "return moves the member output natively",
                );
                // The desktop needs no rewrite: KWin preserves it across
                // the unplug, so only the output send actuates. A redundant
                // desktop write here would be a bug.
                assert.ok(
                    !h.world.membershipWrites.some((w) => w.startsWith("win-b->")),
                    "no redundant desktop rewrite when already homed",
                );
                assert.ok(h.world.viewSwitches.includes("ws2"), "return shows the workspace natively");
                assert.equal(h.world.geometryWrites.length, 0, "return writes no frames");
                assert.ok(
                    !h.world.membershipCalls.slice(memberBefore).some((c) => c.startsWith("win-a->")),
                    "user-moved window is never pulled back",
                );
                assert.deepEqual(
                    h.world.focused,
                    focusedBefore,
                    "policy return never steals focus",
                );
                h.runTimers();
                const feedback = feedbackOf(h);
                const settled = feedback.find((f) => f["op"] === "op-hp-win-b");
                assert.ok(settled !== undefined);
                assert.equal(settled["outcome"], "applied", "member proof settles on the same flight");
                // An identical repeated reply never re-actuates.
                const callsAfter = h.world.membershipCalls.length;
                const outsAfter = h.world.outputWrites.length;
                const viewsAfter = h.world.viewSwitches.length;
                dispatchAndReply(h, rev, {
                    scopes: [scopeRow("out/ws1", "controlled"), scopeRow("out-2/ws2", "controlled")],
                    workspaces: [wsRow("ws1", "out", 0), wsRow("ws2", "out-2", 0)],
                    outputs: [outRow("out", "ws1"), outRow("out-2", "ws2")],
                    windows: [memberRow("win-b", "out-2", "ws2")],
                    transfers: [
                        { op: "op-hp-win-b", kind: "window", win: "win-b", out: "out-2", ws: "ws2", follow: false, held: false },
                        { op: "op-disp-out-2-ws2", kind: "view", win: "", out: "out-2", ws: "ws2", follow: false, held: false },
                    ],
                });
                assert.equal(h.world.membershipCalls.length, callsAfter, "confirmed member never repeats");
                assert.equal(h.world.outputWrites.length, outsAfter, "confirmed output never repeats");
                assert.equal(h.world.viewSwitches.length, viewsAfter, "confirmed view never re-switches");
            } else {
                dispatchAndReply(h, rev, {
                    scopes: [scopeRow("out/ws1", "controlled"), scopeRow("out-2/ws2", "controlled")],
                    workspaces: [wsRow("ws1", "out", 0), wsRow("ws2", "out-2", 0)],
                    outputs: [outRow("out", "ws1"), outRow("out-2", "ws2")],
                    windows: [memberRow("win-b", "out-2", "ws2")],
                    transfers: [],
                });
                assert.equal(h.world.membershipCalls.length, memberBefore, "shared moves nothing on return");
                assert.equal(h.world.outputWrites.length, outputBefore, "shared outputs nothing on return");
            }
            h.handle?.stop();
        });
    }
});

describe("world admission focus on settled floats", () => {
    // D4 newcomer intent (`af-<win>`): a still-plain ordinary float takes
    // the one-shot activation with proof; game/unsafe overlays refuse;
    // command-issued focus keeps the exact legacy float exclusion.
    interface AdmissionSetup {
        state: WorldAdapterState;
        observation: ApplyEnv["observation"];
        reply: Exclude<ReturnType<typeof parseWorldReply>, null>;
        focused: string[];
    }

    function admissionSetup(
        effectId: string,
        observed: { floating: boolean; fullscreen: boolean; maximized: boolean; sticky: boolean; fitExcluded?: boolean },
        floatOrigin: string,
        opts?: {
            siblings?: Array<{ win: string; floating: boolean; fullscreen: boolean; maximized: boolean; sticky: boolean }>;
            replyFloating?: boolean;
        },
    ): AdmissionSetup {
        const state = new WorldAdapterState("test-adapter", "owner-a");
        state.onDiscovery("gen-1");
        state.invalidate();
        const begun = state.beginDispatch(null);
        assert.ok(begun !== null);
        const dispatchObs = state.currentObs;
        const text = JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: "req-1",
            obs: dispatchObs, rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [{
                win: "win-b", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "untouched", member_hold: "",
                frame_op: "",
                floating: opts?.replyFloating ?? true, float_origin: floatOrigin,
            }],
            transfers: [],
            effects: [{ id: effectId, kind: "focus", target: "win-b", rev: "1" }],
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && !isWorldRejection(parsed));
        if (isWorldRejection(parsed) || parsed === null) {
            throw new Error("unreachable");
        }
        const verdict = state.acceptReply(parsed, dispatchObs);
        assert.deepEqual(verdict, { applied: true, reason: "accepted" });
        const ref = {};
        const observation: ApplyEnv["observation"] = {
            mode: "per-output-local",
            topology: [{ out: "out", order: ["ws1"], current: "ws1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
            bindings: [],
            domains: [{
                id: "out", ws: "ws1", bounds: { x: 0, y: 0, w: 800, h: 600 },
                gap: 8, outer: 0, focused: "win-b", readable: true, hold: "",
                adjacent: {}, complete: true,
                windows: [{
                    win: "win-b", ref, out: "out", ws: "ws1",
                    rect: { x: 0, y: 0, w: 100, h: 100 },
                    floating: observed.floating, fitExcluded: observed.fitExcluded ?? true,
                    fullscreen: observed.fullscreen, maximized: observed.maximized,
                    sticky: observed.sticky, fixedAuto: false, fixedSuppress: false,
                    hints: { minW: null, minH: null, maxW: null, maxH: null },
                    minimized: false,
                },
                ...(opts?.siblings ?? []).map((sib) => ({
                    win: sib.win, ref: {}, out: "out", ws: "ws1",
                    rect: { x: 0, y: 0, w: 100, h: 100 },
                    floating: sib.floating,
                    fitExcluded: sib.floating || sib.fullscreen || sib.maximized || sib.sticky,
                    fullscreen: sib.fullscreen, maximized: sib.maximized,
                    sticky: sib.sticky, fixedAuto: false, fixedSuppress: false,
                    hints: { minW: null, minH: null, maxW: null, maxH: null },
                    minimized: false,
                }))],
                closed: [],
            }],
            minimized: [],
            predicate: "both-axes-fixed",
            complete: true,
            gesture: false,
            focusedGlobal: "",
        };
        return { state, observation, reply: parsed, focused: [] };
    }

    function admissionEnv(setup: AdmissionSetup): ApplyEnv {
        const target = (setup.observation.domains[0] as { windows: Array<{ ref: object }> }).windows[0]?.ref as object;
        return {
            state: setup.state,
            observation: setup.observation,
            reply: setup.reply as ApplyEnv["reply"],
            log: () => undefined,
            isStillListed: () => true,
            isActiveWindow: (ref) => ref === target,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws1" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => { throw new Error("float takes no frame write"); },
            setMembership: () => { throw new Error("no membership here"); },
            sendToOutput: () => { throw new Error("no send here"); },
            resolveOutput: () => null,
            resolveDesktop: () => null,
            switchView: () => { throw new Error("no view here"); },
            createDesktop: () => null,
            removeDesktop: () => false,
            focusWindow: (ref) => {
                if (ref === target) {
                    setup.focused.push("win-b");
                    return true;
                }
                return false;
            },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => null,
            desktopById: () => null,
            minDesktops: 2,
        };
    }

    it("focuses a plain ordinary float once with proof and never repeats", () => {
        const setup = admissionSetup("af-win-b", { floating: true, fullscreen: false, maximized: false, sticky: false }, "");
        const result = applyWorldReply(admissionEnv(setup));
        assert.deepEqual(setup.focused, ["win-b"], "admission intent activates the floated newcomer");
        assert.equal(result.effects, 1);
        assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.outcome, "applied");
        // A repeated diff never reapplies: the effect stays spent.
        const again = applyWorldReply(admissionEnv(setup));
        assert.deepEqual(setup.focused, ["win-b"], "no second activation");
        assert.equal(again.effects, 0);
    });

    it("focuses a capacity float exactly like an ordinary float", () => {
        const setup = admissionSetup("af-win-b", { floating: true, fullscreen: false, maximized: false, sticky: false }, "capacity");
        const result = applyWorldReply(admissionEnv(setup));
        assert.deepEqual(setup.focused, ["win-b"], "diagnostic origin changes nothing");
        assert.equal(result.effects, 1);
    });

    for (const overlay of ["fullscreen", "maximized", "sticky"] as const) {
        it(`refuses admission focus on a ${overlay} overlay with no write`, () => {
            const setup = admissionSetup("af-win-b", {
                floating: true,
                fullscreen: overlay === "fullscreen",
                maximized: overlay === "maximized",
                sticky: overlay === "sticky",
            }, "");
            const result = applyWorldReply(admissionEnv(setup));
            assert.deepEqual(setup.focused, [], "overlays never take automatic focus writes");
            assert.equal(result.effects, 0);
            assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.outcome, "unattempted");
            assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.hold, "refused-target");
        });
    }

    it("keeps the legacy float exclusion for command-issued focus", () => {
        const setup = admissionSetup("fx-7", { floating: true, fullscreen: false, maximized: false, sticky: false }, "");
        const result = applyWorldReply(admissionEnv(setup));
        assert.deepEqual(setup.focused, [], "command focus never steers to a float");
        assert.equal(result.effects, 0);
        assert.equal(setup.state.pendingFeedback.find((f) => f.op === "fx-7")?.outcome, "unattempted");
    });

    it("focuses a plain tiled newcomer once with no setter writes", () => {
        const setup = admissionSetup(
            "af-win-b",
            { floating: false, fullscreen: false, maximized: false, sticky: false, fitExcluded: false },
            "",
            { replyFloating: false },
        );
        const env = admissionEnv(setup);
        let setters = 0;
        const counting: typeof env = {
            ...env,
            setGeometry: () => { setters += 1; throw new Error("tiled newcomer takes no frame write"); },
            setMembership: () => { setters += 1; throw new Error("no membership here"); },
            sendToOutput: () => { setters += 1; throw new Error("no send here"); },
            switchView: () => { setters += 1; throw new Error("no view here"); },
        };
        const result = applyWorldReply(counting);
        assert.deepEqual(setup.focused, ["win-b"], "safe ordinary newcomer focuses once");
        assert.equal(result.effects, 1);
        assert.equal(setters, 0, "admission focus writes no geometry/membership/view");
        assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.outcome, "applied");
        const again = applyWorldReply(admissionEnv(setup));
        assert.equal(again.effects, 0, "no second activation");
    });

    for (const overlay of ["fullscreen", "maximized"] as const) {
        it(`refuses admission focus behind a ${overlay} sibling with no write`, () => {
            const setup = admissionSetup(
                "af-win-b",
                { floating: true, fullscreen: false, maximized: false, sticky: false },
                "",
                {
                    siblings: [{
                        win: "win-a",
                        floating: false,
                        fullscreen: overlay === "fullscreen",
                        maximized: overlay === "maximized",
                        sticky: false,
                    }],
                },
            );
            const result = applyWorldReply(admissionEnv(setup));
            assert.deepEqual(setup.focused, [], "never force activation away from a protected overlay");
            assert.equal(result.effects, 0);
            assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.outcome, "unattempted");
            assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.hold, "refused-target");
        });
    }

    it("refuses a game float the core never classified as an ordinary float", () => {
        const setup = admissionSetup(
            "af-win-b",
            { floating: true, fullscreen: false, maximized: false, sticky: false },
            "",
            { replyFloating: false },
        );
        const result = applyWorldReply(admissionEnv(setup));
        assert.deepEqual(setup.focused, [], "observed floating alone never earns activation");
        assert.equal(result.effects, 0);
        assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.outcome, "unattempted");
        assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.hold, "refused-target");
    });

    it("reports native denial with proof and zero setter writes", () => {
        const setup = admissionSetup("af-win-b", { floating: true, fullscreen: false, maximized: false, sticky: false }, "");
        const env = admissionEnv(setup);
        let setters = 0;
        const denying: typeof env = {
            ...env,
            setGeometry: () => { setters += 1; return true; },
            setMembership: () => { setters += 1; return true; },
            sendToOutput: () => { setters += 1; return true; },
            switchView: () => { setters += 1; return true; },
            focusWindow: () => false,
            isActiveWindow: () => false,
        };
        const result = applyWorldReply(denying);
        assert.deepEqual(setup.focused, [], "denied focus writes nothing");
        assert.equal(result.effects, 0);
        assert.equal(setters, 0, "no geometry/membership/view writes around a focus effect");
        assert.equal(setup.state.pendingFeedback.find((f) => f.op === "af-win-b")?.outcome, "refused-proven");
    });
});
