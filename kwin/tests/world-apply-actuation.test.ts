// World actuation identity and proof (D3): the production entry drives
// every scenario below across multiple full report flights with real native
// call counts. Membership/view feedback uses the stable core transfer
// operation id (`op-<cmdSeq>`) exactly, never an invented `send-<win>` or
// `view-<win>` token; possibly-executed attempts never repeat without fresh
// unambiguous proof; Applied settles only on read-back proof; throws and
// partial writes stay ambiguous; proven external input supersedes; creation
// recovers by enumeration against the pre-attempt baseline; raise/stack
// never focus. Native creation follows the Stage 1b contract: only explicit
// core desires execute (at most once per stable op), the same-turn
// before/after delta proves the created id, and receipts persist until the
// core incorporates them. Unbound rows never authorize.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";

interface FakeDesktop {
    id: string;
    name: string;
}
interface FakeWindow {
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullscreen: boolean;
    maximized: boolean;
    sticky: boolean;
    readonly onAllDesktops: boolean;
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
    // Corrupt the first desktop name inside createDesktop (after the
    // native side effect) so only the same-turn post-call readback is
    // unreadable; the test restores the name before the next capture.
    corruptAfterCreate: boolean;
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
    internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
    normalWindow: true,
    frameGeometry: { x, y: 0, width: 200, height: 200 },
    output: { name: out },
    desktops: [ws],
    minimized: false,
    hidden: false,
    fullscreen: false,
    maximized: false,
    sticky: false,
    get onAllDesktops() { return this.sticky; },
    keepAbove: false,
    keepBelow: false,
    minSize: null,
    maxSize: null,
});

function makeHarness(mode: unknown = "per-output-local"): Harness {
    const faults: Faults = { deferMembership: false, throwMembership: null, createFaults: {}, signalOnCreate: false, geometryEchoSignal: false, corruptAfterCreate: false };
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
            if (faults.corruptAfterCreate) {
                (world.desktops[0] as unknown as Record<string, unknown>)["name"] = 42;
            }
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
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({ v: 3, outcome: "queued", gen: "gen-1",
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 3, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 3, outcome: "pending", gen: "gen-1" }));
                }            }
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

const DISABLED_PRESENTATION = {
    foreground: "",
    border: { visible: false, reason: "disabled", inner: null, outer: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null },
    pager: [],
};

const BOOTSTRAP = (gen: string, req: string): string => JSON.stringify({
    v: 3, adapter: "broker-1", gen, req, obs: "0", rev: "0",
    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
    windows: [], transfers: [], effects: [], result: null,
    gestureSettled: null,  operationSettled: [], nativeCreations: [], warpCursor: null,
    presentation: DISABLED_PRESENTATION,
});

function discover(h: Harness, gen = "gen-1"): void {
    const call = h.calls.filter((c) => c.method === "DiscoverWorld").pop() as CapturedCall;
    const body = JSON.parse(call.payload);
    call.callback(BOOTSTRAP(gen, body.req));
    const auto = h.calls.filter((c) => c.method === "DescribeWorld").pop() as CapturedCall;
    const autoBody = JSON.parse(auto.payload) as Record<string, unknown>;
    const report = autoBody["report"] as Record<string, unknown>;
    assert.equal(report["command"], undefined, "inner reports never carry a command");
    auto.callback(acceptedReply(gen, autoBody["source"] as string, autoBody["nativeInvalid"] as string, "0"));
    answerTake(h, null);
}

// The broker files every queued command; the fake transport answers queued
// immediately so the entry drains on its next fresh capture, exactly like
// the production wake path (our own queue dispatches our own capture).
function answerQueue(h: Harness): void {
    for (const c of h.calls.filter((c) => c.method === "QueueWorldCommand")) {
        const marked = c as CapturedCall & { answered?: boolean };
        if (marked.answered === true) {
            continue;
        }
        marked.answered = true;
        const body = JSON.parse(c.payload) as Record<string, unknown>;
        c.callback(JSON.stringify({
            v: 3, outcome: "queued", gen: "gen-1",
            client: body["client"], clientCmd: body["clientCmd"], globalCmd: "1",
        }));
    }
}

// Terminal take collection: pending by default (the settlement stays
// re-readable server-side); a staged result answers ready exactly once.
// Observation-only captures file no Take (clientCmd null), so a missing
// Take is a no-op, not a failure.
function answerTake(h: Harness, result: Record<string, unknown> | null): void {
    const take = [...h.calls].reverse().find((c) => c.method === "TakeCommandResult");
    if (take === undefined) {
        return;
    }
    take.callback(JSON.stringify(result === null
        ? { v: 3, outcome: "pending", gen: "gen-1" }
        : { v: 3, outcome: "ready", gen: "gen-1", globalCmd: "1", result }));
}

function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    // Bound rows the broker always ships alongside transfers (fused
    // here, matching the fake natives): adapter-side resolution never
    // falls back to native string equality. Tests with a second output
    // resolve its fused row the same way.
    const fused = (id: string, out: string, order: number): Record<string, unknown> => (
        { id, out, bind: id, scope: out, order, mode: "tiled", exist: true, hold: "" }
    );
    const workspaces = [fused("ws1", "out", 0), fused("ws2", "out", 1), fused("ws3", "out", 2), fused("ws2", "out-2", 0)];
    return JSON.stringify({
        v: 3, adapter: "broker-1", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "0",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces,
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        result: null,
        ...extra,
    });
}

function lastDescribe(h: Harness): CapturedCall {
    const found = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(found !== undefined, "expected DescribeWorld");
    return found as CapturedCall;
}

// Dispatch one debounced report (timers), file any queued head, and reply
// accepted with rev from the counter. Returns the envelope payload the
// reply answered.
function dispatchAndReply(h: Harness, rev: { n: number }, extra: Record<string, unknown> = {}): Record<string, unknown> {
    h.runTimers();
    answerQueue(h);
    const call = lastDescribe(h);
    const body = JSON.parse(call.payload) as Record<string, unknown>;
    rev.n += 1;
    call.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, String(rev.n), extra));
    answerTake(h, null);
    return body;
}

// Answer an idle observation-only flight with a quiet accept so the next
// leg starts idle: a reply to an already-settled flight is ignored by its
// settled guard, so this is safe either way.
function drain(h: Harness, rev: { n: number }): void {
    dispatchAndReply(h, rev, {});
    h.runTimers();
}

// Wire-exactness oracle for the queue lane (replaces the deleted verdict
// transport): every filed command uses the exact v2 vocabulary with no
// invented routing keys.
const KNOWN_OPS = new Set([
    "focus", "send", "switch-view", "select-workspace", "select-numbered",
    "select-relative", "select-previous", "select-trailing", "send-relative",
    "send-numbered", "set-workspace-tiling", "toggle-workspace-tiling", "migrate-workspace",
    "focus-directional", "move-directional", "send-directional",
    "migrate-directional", "operation",
]);

function noInventedQueueOps(h: Harness): void {
    for (const c of h.calls.filter((c) => c.method === "QueueWorldCommand")) {
        const body = JSON.parse(c.payload) as Record<string, unknown>;
        const command = body["command"] as Record<string, unknown>;
        assert.ok(KNOWN_OPS.has(command["op"] as string), `unknown queue op ${command["op"]}`);
        assert.ok(!("floatSubject" in command), "no local routing key on the wire");
        assert.ok(!("from" in command && command["op"] === "migrate-directional"), "no legacy migrate keys");
    }
}

const winRow = (winId: string, ws: string): Record<string, unknown> => ({
    win: winId, out: "out", ws,
    alloc: null, frame: null,
    frame_auth: "controlled", frame_hold: "",
    member_auth: "controlled", member_hold: "",
    frame_op: "",
    clear_op: "",
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
    clear_op: "",
    floating: false, float_origin: "",
});

// Explicit untouched floating lifetime: authorizes nothing.
const untouchedRow = (winId: string, ws: string): Record<string, unknown> => ({
    win: winId, out: "out", ws,
    alloc: null, frame: null,
    frame_auth: "untouched", frame_hold: "",
    member_auth: "untouched", member_hold: "",
    frame_op: "",
    clear_op: "",
    floating: false, float_origin: "",
});

// Explicit native creation desires (Stage 1b contract): the core authorizes
// exactly one native create per outstanding stable op. An unbound row
// without a desire row authorizes nothing.
const desire = (op: string, ws: string, out = "out"): Record<string, unknown> => ({ op, ws, out });
const unboundRow = (id: string, out = "out", order = 3): Record<string, unknown> => (
    { id, out, bind: "", scope: out, order, mode: "tiled", exist: true, hold: "" }
);
const boundRow = (id: string, bind: string, out = "out", order = 3): Record<string, unknown> => (
    { id, out, bind, scope: out, order, mode: "tiled", exist: true, hold: "" }
);
// Retained creation receipts riding the last dispatched report lane.
function lastCreationAttempts(h: Harness): Array<Record<string, unknown>> {
    const call = lastDescribe(h);
    const body = JSON.parse(call.payload) as Record<string, unknown>;
    const report = (body["report"] ?? body) as Record<string, unknown>;
    const attempts = report["creationAttempts"];
    assert.ok(Array.isArray(attempts), "report carries the creation lane");
    return attempts as Array<Record<string, unknown>>;
}

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
        // Read-back proof settles on the same flight under the stable core
        // op: no second setter above proves it. Verdict transport is
        // deleted; the stable transfer op rides the queue lane instead.
        noInventedQueueOps(h);
        // The native side has converged meanwhile: the next flight confirms
        // with read-back proof and issues no second setter call.
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "confirmed without repeating the setter");
        h.runTimers();
        noInventedQueueOps(h);
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
        noInventedQueueOps(h);
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
        noInventedQueueOps(h);
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
        // Ambiguity holds locally (never a second attempt above); verdict
        // transport is deleted.
        noInventedQueueOps(h);
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
        noInventedQueueOps(h);
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
        // The throw stays ambiguous (never a second attempt below), never
        // refused-proven; verdict transport is deleted.
        noInventedQueueOps(h);
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
        // Still ambiguous (never a second attempt below); verdict transport
        // is deleted.
        noInventedQueueOps(h);
        // Late native convergence completes it with proof, no new attempt.
        h.rawApplyMembership.get("win-a")?.([h.world.desktops.find((d) => d.id === "ws2") as FakeDesktop]);
        h.fire("windowActivated");
        dispatchAndReply(h, rev, {
            windows: [winRow("win-a", "ws2")],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: true }],
        });
        assert.equal(h.world.membershipCalls.length, 1, "proof confirms without a second attempt");
        h.runTimers();
        noInventedQueueOps(h);
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
        // Read-back proof wins over the throw (no new attempt above);
        // verdict transport is deleted.
        noInventedQueueOps(h2);
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
        // Synchronous proof settles on the same flight (one switch above);
        // verdict transport is deleted.
        noInventedQueueOps(h);
        // Identical view target confirms without a second switch.
        dispatchAndReply(h, rev, {
            outputs: [{ out: "out", view: "ws2", mode: "tiled", hold: "" }],
            transfers: [viewTransfer(false)],
        });
        assert.deepEqual(h.world.viewSwitches, ["ws2"], "confirmed view never re-switches");
        h.runTimers();
        noInventedQueueOps(h);
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
        // No synthetic follow identity rides the queue lane (checked above);
        // verdict transport is deleted.
        noInventedQueueOps(h);
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
        noInventedQueueOps(h);
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
        // External input settles superseded locally (never fought above);
        // verdict transport is deleted.
        noInventedQueueOps(h);
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
        both.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, String(rev.n), {
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
            clear_op: "",
            floating: false, float_origin: "",
        });
        dispatchAndReply(h2, rev2, { windows: [geom("win-a", 11), geom("win-b", 22)] });
        assert.ok(h2.world.geometryWrites.some((w) => w.startsWith("win-a@11,11")), "first geometry applies");
        assert.ok(!h2.world.geometryWrites.some((w) => w.startsWith("win-b@22,22")), "input invalidation stops the rest");
    });

    it("creates only explicit desires at most once with a causal created id", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const unbound = unboundRow("wl-1");
        // An unbound row alone authorizes nothing: no desire, no create.
        dispatchAndReply(h, rev, { workspaces: [unbound] });
        assert.equal(h.world.createCalls, 0, "unbound rows never authorize creation");
        // The explicit core desire executes exactly once with the causal
        // same-turn delta as the created id.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h.world.createCalls, 1, "one creation flight per stable op");
        assert.deepEqual(h.world.createArgs[0], [3, "wl-1"], "explicit zero-based position plus logical name");
        assert.equal(h.world.created.length, 1, "void native created, same-turn delta proves");
        const created = h.world.created[0] as string;
        // The receipt rides the next report lane until the core incorporates it.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h.world.createCalls, 1, "spent op never re-executes");
        const attempts = lastCreationAttempts(h);
        assert.equal(attempts.length, 1, "one retained receipt");
        assert.equal(attempts[0]?.["op"], "op-native-1");
        assert.equal(attempts[0]?.["created"], created, "causal created id preserved");
        // The core publishes the bind: the receipt incorporates with no
        // second creation, and the following report rides clean.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [boundRow("wl-1", created)] });
        assert.equal(h.world.createCalls, 1, "incorporated desire never repeats");
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [boundRow("wl-1", created)] });
        const after = lastCreationAttempts(h);
        assert.equal(after.length, 0, "incorporated receipt retires");
    });

    it("records an uncorrelated receipt for a no-op without repeating", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "noop";
        const unbound = unboundRow("wl-1");
        dispatchAndReply(h, rev, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h.world.createCalls, 1);
        assert.equal(h.world.created.length, 0, "no-op created nothing");
        // Uncorrelated (created null): the receipt rides pending, the op
        // never re-executes even while the desire is retained.
        h.fire("desktopsChanged");
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h.world.createCalls, 1, "spent op never repeats blindly");
        const attempts = lastCreationAttempts(h);
        assert.equal(attempts.length, 1, "receipt preserved");
        assert.equal(attempts[0]?.["created"], null, "no delta claims no id");
        assert.equal(attempts[0]?.["setter"], "accepted", "void acceptance stays pending, never a refusal latch");
    });

    it("records a throwing create with its side-effect id and never repeats", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "throw-mutating";
        const unbound = unboundRow("wl-1");
        dispatchAndReply(h, rev, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h.world.createCalls, 1);
        assert.equal(h.world.created.length, 1, "throw happened after the side-effect");
        const created = h.world.created[0] as string;
        delete h.faults.createFaults["wl-1"];
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h.world.createCalls, 1, "throw+side-effect proves, never repeats");
        const attempts = lastCreationAttempts(h);
        assert.equal(attempts[0]?.["created"], created, "side-effect id preserved causally");
        assert.equal(attempts[0]?.["setter"], "threw");
        const h2 = makeHarness();
        discover(h2);
        const rev2 = { n: 0 };
        h2.handle?.requestWorkspaceRelative(1);
        h2.faults.createFaults["wl-1"] = "throw-clean";
        dispatchAndReply(h2, rev2, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h2.world.createCalls, 1);
        assert.equal(h2.world.created.length, 0, "clean throw created nothing");
        delete h2.faults.createFaults["wl-1"];
        h2.fire("desktopsChanged");
        dispatchAndReply(h2, rev2, {
            workspaces: [unbound],
            nativeCreations: [desire("op-native-1", "wl-1")],
        });
        assert.equal(h2.world.createCalls, 1, "clean throw never repeats blindly");
        const attempts2 = lastCreationAttempts(h2);
        assert.equal(attempts2[0]?.["created"], null);
        assert.equal(attempts2[0]?.["setter"], "threw");
    });

    it("never shares one native desktop between two tokens in the same reply", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        // Second desire clean-throws: it must not adopt the first desire's desktop.
        h.faults.createFaults["wl-b"] = "throw-clean";
        const unboundA = unboundRow("wl-a", "out", 3);
        const unboundB = unboundRow("wl-b", "out", 4);
        const desires = [desire("op-native-1", "wl-a"), desire("op-native-2", "wl-b")];
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB], nativeCreations: desires });
        assert.equal(h.world.createCalls, 2, "each desire attempts once from its own baseline");
        assert.deepEqual(h.world.createArgs[0], [3, "wl-a"], "first desire appends at count");
        assert.deepEqual(h.world.createArgs[1], [4, "wl-b"], "second desire baselines past the first creation");
        assert.equal(h.world.created.length, 1, "only the first desire created");
        assert.ok(h.logs.some((l) => l.includes(" mk=1 ")), "only the proven desire counts, no steal");
        h.runTimers();
        noInventedQueueOps(h);
        // Next flight: both ops spent, neither repeats.
        delete h.faults.createFaults["wl-b"];
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB], nativeCreations: desires });
        assert.equal(h.world.createCalls, 2, "spent ops never repeat, bound token never repeats");
        const names = h.world.desktops.map((d) => d.name);
        assert.equal(names.filter((n) => n === "Desktop 4").length, 1, "proven token renamed to its enumerated position");
        assert.ok(!names.includes("wl-b"), "no fabricated binding");
    });

    it("binds two successful creates in the same reply independently", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const unboundA = unboundRow("wl-a", "out", 3);
        const unboundB = unboundRow("wl-b", "out", 4);
        const desires = [desire("op-native-1", "wl-a"), desire("op-native-2", "wl-b")];
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB], nativeCreations: desires });
        assert.equal(h.world.createCalls, 2);
        assert.equal(h.world.created.length, 2, "two natives created");
        assert.ok(h.logs.some((l) => l.includes(" mk=2 ")), "both creations prove same-flight");
        const names = h.world.desktops.map((d) => d.name);
        assert.ok(names.includes("Desktop 4") && names.includes("Desktop 5"), "each proven native carries its friendly position name");
        assert.ok(!names.includes("wl-a") && !names.includes("wl-b"), "token names leave the inventory after binding");
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB], nativeCreations: desires });
        assert.equal(h.world.createCalls, 2, "no perpetual ambiguity, no repeats");
        // Both receipts ride causally until the core incorporates them.
        const attempts = lastCreationAttempts(h);
        assert.equal(attempts.length, 2, "both receipts preserved");
        assert.ok(attempts.every((a) => a["created"] !== null), "both causally correlated");
    });

    it("never claims an unrelated user-appended desktop as its own", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "noop";
        const unbound = unboundRow("wl-1");
        const desires = [desire("op-native-1", "wl-1")];
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1);
        // The user appends an unrelated desktop natively (different id).
        h.world.desktops.push(desk("user-9", "User 9"));
        delete h.faults.createFaults["wl-1"];
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "spent op never re-executes, user desktop never claimed");
        h.runTimers();
        noInventedQueueOps(h);
        // Even healthy, the spent op never re-attempts; the receipt stays
        // uncorrelated (the later user desktop is core-side evidence, never
        // an adapter claim).
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "still spent, still no repeat");
        const attempts = lastCreationAttempts(h);
        assert.equal(attempts[0]?.["created"], null, "adapter claims no id it did not prove");
        assert.ok(!h.world.desktops.map((d) => d.name).includes("wl-1"), "no fabricated binding");
    });

    it("holds an unreadable baseline unattempted and proceeds once readable", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const unbound = unboundRow("wl-1");
        const desires = [desire("op-native-1", "wl-1")];
        // Corrupt one native name read: the named snapshot is incomplete,
        // while id-only observation capture still succeeds.
        const ws1 = h.world.desktops[0] as unknown as Record<string, unknown>;
        const saved = ws1["name"];
        ws1["name"] = 42;
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 0, "incomplete inventory attempts nothing");
        assert.equal(h.world.created.length, 0);
        h.runTimers();
        // Read-only failure never claims a native attempt (zero creates
        // above); verdict transport is deleted.
        drain(h, rev);
        ws1["name"] = saved;
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "fresh readable baseline permits the first native attempt");
        assert.equal(h.world.desktops.filter((d) => d.name === "Desktop 4").length, 1, "proven desktop carries its friendly position name");
        // The op spends on the readable attempt, so a retained desire never duplicates.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "spent op never duplicates");
    });

    it("defers the receipt when the post-call readback is unavailable, then completes once uncorrelated", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const unbound = unboundRow("wl-1");
        const desires = [desire("op-native-1", "wl-1")];
        // The native call executes, but the same-turn post-call readback
        // is unreadable: no receipt is emitted yet (never `after: []`, no
        // `before` copy, no invented causal id).
        h.faults.corruptAfterCreate = true;
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "executed exactly once");
        assert.equal(h.world.created.length, 1, "side effect happened despite the unreadable readback");
        assert.equal(lastCreationAttempts(h).length, 0, "unavailable read emits no false evidence");
        h.faults.corruptAfterCreate = false;
        (h.world.desktops[0] as unknown as Record<string, unknown>)["name"] = "WS1";
        // The next capture completes the preserved evidence once against
        // the first successfully observed full native order, uncorrelated.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "completion never re-creates");
        const attempts = lastCreationAttempts(h);
        assert.equal(attempts.length, 1, "receipt completed once");
        assert.equal(attempts[0]?.["created"], null, "later turn never claims a causal id");
        assert.deepEqual(attempts[0]?.["after"], h.world.desktops.map((d) => d.id), "after is the observed order, never fabricated");
        assert.equal(attempts[0]?.["setter"], "accepted");
        assert.ok(!h.world.desktops.map((d) => d.name).includes("Desktop 4"), "deferred proof never renames");
        // The retained desire still never re-executes; the receipt rides
        // until the core incorporates it.
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "spent op never repeats");
        assert.equal(lastCreationAttempts(h).length, 1, "receipt rides until incorporation");
    });

    it("keeps a spent receipt without another native call when proof lands late", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.createFaults["wl-1"] = "noop";
        const unbound = unboundRow("wl-1");
        const desires = [desire("op-native-1", "wl-1")];
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1);
        // The desktop lands late natively (outside visibility timing): the
        // spent op never fires a second native call for it.
        h.world.desktops.push(desk("delayed-1", "wl-1"));
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1);
        assert.equal(h.logs.filter((l) => l.includes(" mk=1 ")).length, 0, "no causal proof, no mk count");
        h.runTimers();
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unbound], nativeCreations: desires });
        assert.equal(h.world.createCalls, 1, "still exactly one native call");
        const attempts = lastCreationAttempts(h);
        assert.equal(attempts[0]?.["created"], null, "late desktop never rewrites the spent receipt");
    });

    it("survives a synchronous desktop signal during creation without duplication", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        h.faults.signalOnCreate = true;
        const unboundA = unboundRow("wl-a", "out", 3);
        const unboundB = unboundRow("wl-b", "out", 4);
        const desires = [desire("op-native-1", "wl-a"), desire("op-native-2", "wl-b")];
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB], nativeCreations: desires });
        assert.ok(h.world.createCalls >= 1, "creation attempted despite reentrant invalidation");
        assert.ok(h.world.createCalls <= 2, "reentrancy never duplicates within one flight");
        h.faults.signalOnCreate = false;
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB], nativeCreations: desires });
        h.fire("desktopsChanged");
        dispatchAndReply(h, rev, { workspaces: [unboundA, unboundB], nativeCreations: desires });
        assert.equal(h.world.createCalls, 2, "each stable op executes exactly once, then converges");
        const boundNames = h.world.desktops.map((d) => d.name);
        const aCount = boundNames.filter((n) => n === "Desktop 4").length;
        const bCount = boundNames.filter((n) => n === "Desktop 5").length;
        assert.ok(aCount <= 1 && bCount <= 1, "no token created twice");
    });

    it("removes only provably empty workspaces and keeps referenced ones", () => {
        const h = makeHarness();
        discover(h);
        const rev = { n: 0 };
        h.handle?.requestWorkspaceRelative(1);
        const absent = (id: string): Record<string, unknown> => ({
            id, out: "out", bind: id, scope: "out", order: 2, mode: "tiled", exist: false, hold: "",
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
        // Raise/stack never steer focus (zero focused above) and never
        // spend as attempts; verdict transport is deleted.
        noInventedQueueOps(h);
        // The second press queues behind the in-flight feedback-carrier
        // report without invalidating it, so that flight stays fresh and
        // the focus effect applies on its own round trip; the queued press
        // then drains after the acceptance.
        h.handle?.requestWorkspaceRelative(-1);
        dispatchAndReply(h, rev, {
            effects: [{ id: "fx-11", kind: "focus", target: "win-b", rev: "2" }],
        });
        assert.deepEqual(h.world.focused, ["win-b"], "focus applies once with proof");
        h.runTimers();
        // Focus applies once with proof (focused above); verdict transport
        // is deleted.
        noInventedQueueOps(h);
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
            dispatchWatermark: overrides.state.currentInvalid,
            takeResult: null,
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
        state.onDiscovery("gen-1", "broker-1");
        state.invalidate();
        const flight = state.beginCapture("req-1", null);
        assert.ok(flight !== null);
        const dispatchObs = flight.watermark;
        const text = JSON.stringify({
            v: 3, adapter: "broker-1", gen: "gen-1", req: "req-1",
            obs: dispatchObs, rev: "1", ready: true, consumed: false,
            source: "req-1",
            nativeInvalid: "1",
            clock: "0",
            gestureSettled: null, gestureAssoc: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                clear_op: "",
                floating: false, float_origin: "",
            }],
            transfers: [], effects: [], result: null,
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed));
        if (!isCoreWorldReply(parsed)) {
            return;
        }
        const verdict = state.acceptReply(parsed, "req-1", flight.watermark);
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
                    maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
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
            screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
            cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
            focusFollowsCursor: false, cursorWarp: false,
            switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
            presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
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
        id, out, bind: id, scope: out, order, mode: "tiled", exist: true, hold: "",
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
        clear_op: "",
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
                // View proof settles on the same flight (one switch above);
                // verdict transport is deleted.
                noInventedQueueOps(h);
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
                // Member proof settles on the same flight (moves above);
                // verdict transport is deleted.
                noInventedQueueOps(h);
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
        logs: string[];
    }

    function admissionSetup(
        effectId: string,
        observed: { floating: boolean; fullscreen: boolean; maximized: boolean; sticky: boolean; fitExcluded?: boolean },
        floatOrigin: string,
        opts?: {
            siblings?: Array<{ win: string; floating: boolean; fullscreen: boolean; maximized: boolean; sticky: boolean }>;
            replyFloating?: boolean;
            observedFixedAuto?: boolean;
            observedFixedSuppress?: boolean;
            logs?: string[];
        },
    ): AdmissionSetup {
        const state = new WorldAdapterState("test-adapter", "owner-a");
        state.onDiscovery("gen-1", "broker-1");
        state.invalidate();
        const flight = state.beginCapture("req-1", null);
        assert.ok(flight !== null);
        const dispatchObs = flight.watermark;
        const text = JSON.stringify({
            v: 3, adapter: "broker-1", gen: "gen-1", req: "req-1",
            obs: dispatchObs, rev: "1", ready: true, consumed: false,
            source: "req-1",
            nativeInvalid: "1",
            clock: "0",
            gestureSettled: null, gestureAssoc: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [{
                win: "win-b", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "untouched", member_hold: "",
                frame_op: "",
                clear_op: "",
                floating: opts?.replyFloating ?? true, float_origin: floatOrigin,
            }],
            transfers: [],
            effects: [{ id: effectId, kind: "focus", target: "win-b", rev: "1" }], result: null,
        });
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed));
        if (!isCoreWorldReply(parsed)) {
            throw new Error("unreachable");
        }
        const verdict = state.acceptReply(parsed, "req-1", flight.watermark);
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
                    sticky: observed.sticky, fixedAuto: opts?.observedFixedAuto === true, fixedSuppress: opts?.observedFixedSuppress === true, nativeWs: null,
                    hints: { minW: null, minH: null, maxW: null, maxH: null },
                    minimized: false,
                },
                ...(opts?.siblings ?? []).map((sib) => ({
                    win: sib.win, ref: {}, out: "out", ws: "ws1",
                    rect: { x: 0, y: 0, w: 100, h: 100 },
                    floating: sib.floating,
                    fitExcluded: sib.floating || sib.fullscreen || sib.maximized || sib.sticky,
                    fullscreen: sib.fullscreen, maximized: sib.maximized,
                    sticky: sib.sticky, fixedAuto: false, fixedSuppress: false, nativeWs: null,
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
            screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
            cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
            focusFollowsCursor: false, cursorWarp: false,
            switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
            presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
        };
        return { state, observation, reply: parsed, focused: [], logs: opts?.logs ?? [] };
    }

    function admissionEnv(setup: AdmissionSetup): ApplyEnv {
        const target = (setup.observation.domains[0] as { windows: Array<{ ref: object }> }).windows[0]?.ref as object;
        return {
            state: setup.state,
            observation: setup.observation,
            reply: setup.reply as ApplyEnv["reply"],
            dispatchWatermark: setup.state.currentInvalid,
            takeResult: null,
            log: (m) => { setup.logs.push(m); },
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
        assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
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
            assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
                    });
    }

    it("keeps the legacy float exclusion for command-issued focus", () => {
        const setup = admissionSetup("fx-7", { floating: true, fullscreen: false, maximized: false, sticky: false }, "");
        const result = applyWorldReply(admissionEnv(setup));
        assert.deepEqual(setup.focused, [], "command focus never steers to a float");
        assert.equal(result.effects, 0);
        assert.equal(setup.state.isDispatched("fx-7"), false, "effects never dispatch-record; at-most-once comes from the spent set");
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
        assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
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
            assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
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
        assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
            });

    it("stays mechanical on a fabricated automatic fixed-size float admission", () => {
        // The core withholds and spends automatic fixed-size admission, so
        // a fabricated af- with fixedAuto is unreachable in production.
        // The adapter stays mechanical here and would activate the
        // still-plain ordinary float; the real no-activation proof lives
        // on the retained-Engine route.
        const setup = admissionSetup(
            "af-win-b",
            { floating: true, fullscreen: false, maximized: false, sticky: false },
            "",
            { observedFixedAuto: true, observedFixedSuppress: false },
        );
        const env = admissionEnv(setup);
        let setters = 0;
        const counting: typeof env = {
            ...env,
            setGeometry: () => { setters += 1; return true; },
            setMembership: () => { setters += 1; return true; },
            sendToOutput: () => { setters += 1; return true; },
            switchView: () => { setters += 1; return true; },
        };
        const result = applyWorldReply(counting);
        assert.deepEqual(setup.focused, ["win-b"], "mechanical adapter activates the fabricated intent");
        assert.equal(result.effects, 1);
        assert.equal(setters, 0, "admission focus writes no geometry/membership/view");
        assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
                assert.ok(setup.logs.some((l) => l.includes("automatic-focus-attempt")), "mechanical admission attempts the setter");
    });

    it("keeps suppressed-tile admission focus with no geometry writes", () => {
        // Explicit fixed-suppress tile stays tiled locally and keeps the
        // ordinary tiled-newcomer one-shot activation.
        const setup = admissionSetup(
            "af-win-b",
            { floating: false, fullscreen: false, maximized: false, sticky: false, fitExcluded: false },
            "",
            { replyFloating: false, observedFixedAuto: true, observedFixedSuppress: true },
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
        assert.deepEqual(setup.focused, ["win-b"], "suppressed tile keeps admission activation");
        assert.equal(result.effects, 1);
        assert.equal(setters, 0);
        assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
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
        assert.equal(setup.state.isDispatched("af-win-b"), false, "effects never dispatch-record; at-most-once comes from the spent set");
    });
});
