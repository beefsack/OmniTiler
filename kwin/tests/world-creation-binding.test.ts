import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop { id: string; name: string }
const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });

interface FakeSignal {
    readonly handlers: Array<(payload?: unknown) => void>;
    readonly signal: {
        connect: (handler: (payload?: unknown) => void) => void;
        disconnect: (handler: (payload?: unknown) => void) => void;
    };
}

function makeSignal(): FakeSignal {
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

function makeTwoOutput(opts: { throwOnce?: boolean } = {}) {
    const wa1 = desk("wa-1", "wa-1");
    const wb1 = desk("wb-1", "wb-1");
    const world = {
        screens: [
            { name: "out-primary", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
            { name: "out-secondary", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } },
        ],
        desktops: [wa1, wb1] as FakeDesktop[],
        wins: [] as Array<Record<string, unknown>>,
        activeScreen: { name: "out-secondary" },
        currentByOut: new Map<string, FakeDesktop>([["out-primary", wa1], ["out-secondary", wb1]]),
        created: [] as string[],
        throwArmed: opts.throwOnce === true,
    };
    const calls: Array<{ method: string; payload: string; callback: (r: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const signals: Record<string, FakeSignal> = {};
    for (const name of [
        "windowAdded",
        "windowRemoved",
        "windowActivated",
        "desktopsChanged",
        "screensChanged",
        "currentDesktopChanged",
        "activeScreenChanged",
    ] as const) {
        signals[name] = makeSignal();
    }
    const surface: Record<string, unknown> = {
        windowList: () => world.wins,
        screens: world.screens,
        get desktops() { return world.desktops; },
        activeScreen: world.activeScreen,
        activeWindow: null,
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (out: { name: string }) => world.currentByOut.get(out.name),
        setCurrentDesktopForScreen: (d: FakeDesktop, out: { name: string }) => { world.currentByOut.set(out.name, d); },
        sendClientToScreen: () => undefined,
        createDesktop: (...args: ReadonlyArray<unknown>) => {
            if (world.throwArmed) {
                world.throwArmed = false;
                throw new Error("Insufficient arguments");
            }
            const name = args[1] as string;
            const id = `native-${world.desktops.length + 1}`;
            world.desktops.push(desk(id, name));
            world.created.push(id);
        },
        removeDesktop: () => undefined,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, cb) => { calls.push({ method, payload, callback: cb }); },
        scheduleOnce: (delay, cb) => { timers.push({ delay, cb }); return () => undefined; },
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.ok(handle !== null);
    const runTimers = () => { for (let i = 0; i < timers.length; i += 1) { const t = timers[i] as { delay: number; cb: () => void }; if (t.delay <= 120) { timers.splice(i, 1); i -= 1; t.cb(); } } };
    const lastCall = (m: string) => { const f = [...calls].reverse().find((c) => c.method === m); assert.ok(f !== undefined); return f as { method: string; payload: string; callback: (r: unknown) => void }; };
    const fire = (name: string, payload?: unknown): void => {
        for (const handler of [...(signals[name] as FakeSignal).handlers]) {
            handler(payload);
        }
    };
    return { world, calls, handle, runTimers, lastCall, fire };
}

function answerHandshake(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse(found.payload) as { req: string }).req;
    found.callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/settings");
    const body = JSON.parse(found.payload) as Record<string, unknown>;
    found.callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

const ACC = (gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string => {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation, the native watermark, and scripted
    // obs/rev advancing together (separate from nativeInvalid); the
    // terminal result stays null (the take lane serves it
    // requester-only).
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null,
        closeFocus: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
        ...extra,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
};

function discover(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies.
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
    const auto = h.lastCall("DescribeWorld");
    const b = JSON.parse(auto.payload) as { source: string; nativeInvalid: string };
    auto.callback(ACC(gen, b.source, b.nativeInvalid, "0"));
}

// Host-owned publication oracle: the adapter reports observed native
// order plus the current native per output in `native[]` (never logical
// views: the core derives receipts from its own bindings, and v4 carries
// no logical views lane at all). Core-published binds are proven by
// incorporation behavior (receipt retires, no re-create).
function nativeOf(h: ReturnType<typeof makeTwoOutput>): Array<{ out: string; order: string[]; current: string }> {
    return (JSON.parse(h.lastCall("DescribeWorld").payload) as { report: { native: Array<{ out: string; order: string[]; current: string }> } }).report.native;
}
function assertNoViews(h: ReturnType<typeof makeTwoOutput>): void {
    const report = JSON.parse(h.lastCall("DescribeWorld").payload) as { report: Record<string, unknown> };
    assert.ok(!("views" in report.report), "v4 carries no logical views lane");
}

// Retained creation receipts riding the last dispatched report lane (the
// lane is omitted when empty, never nulled: omission backfills the
// observation-only default).
function attemptsOf(h: ReturnType<typeof makeTwoOutput>): Array<Record<string, unknown>> {
    const payload = JSON.parse(h.lastCall("DescribeWorld").payload) as { report: { creationAttempts?: Array<Record<string, unknown>> } };
    const attempts = payload.report.creationAttempts ?? [];
    assert.ok(Array.isArray(attempts), "populated creation lane rides an array");
    return attempts;
}

const DESIRE = (op: string, ws: string, out: string): Record<string, unknown> => ({ op, ws, out });

function dispatchObserve(h: ReturnType<typeof makeTwoOutput>): { source: string; nativeInvalid: string } {
    // Signal-driven observation round (production path): a native inventory
    // event invalidates and the debounce dispatches a fresh capture. Press
    // intents queue instead of dispatching, so they cannot drive the round.
    h.fire("desktopsChanged");
    h.runTimers();
    return JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
}

// Show a native desktop on an output (ordinary native view input):
// incorporation is observed through receipt retirement plus native
// order/current evidence, so observing it requires the created desktop to
// be shown.
function showNative(h: ReturnType<typeof makeTwoOutput>, out: string, nativeId: string): void {
    const ref = h.world.desktops.find((d) => d.id === nativeId);
    assert.ok(ref !== undefined, `native ${nativeId} exists`);
    h.world.currentByOut.set(out, ref);
}

describe("creation binding handoff", () => {
    it("executes one explicit desire and reports the causal receipt without publishing", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        assert.equal(h.world.created.length, 1);
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "proven desktop renamed to its native one-based position");
        // No core bind yet: the adapter publishes native order/current with
        // no logical views lane, and claims only the causal receipt, never
        // a binding.
        showNative(h, "out-secondary", native.id);
        dispatchObserve(h);
        assertNoViews(h);
        const pendingNative = nativeOf(h).find((b) => b.out === "out-secondary");
        assert.ok(pendingNative !== undefined);
        assert.ok(pendingNative.order.includes(native.id), "created native rides the observed order");
        assert.equal(pendingNative.current, native.id, "shown native rides the current");
        const receipts = attemptsOf(h);
        assert.equal(receipts.length, 1, "one retained receipt rides the report lane");
        assert.equal(receipts[0]?.["op"], "op-native-1");
        assert.equal(receipts[0]?.["created"], native.id, "causal created id preserved");
        // The core publishes the bind: the receipt incorporates and the
        // view publishes with no second creation.
        const r2 = JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: native.id, scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1, "incorporation never re-creates");
        dispatchObserve(h);
        // The core bind incorporates the receipt (it retires with no second
        // creation); the adapter keeps reporting native order/current with
        // no logical views lane, never a logical receipt.
        assertNoViews(h);
        const row = nativeOf(h).find((b) => b.out === "out-secondary");
        assert.ok(row !== undefined);
        assert.ok(row.order.includes(native.id), `expected created native in order, got ${JSON.stringify(nativeOf(h))}`);
        assert.equal(row.current, native.id);
        assert.equal(attemptsOf(h).length, 0, "incorporated receipt retires");
        h.handle?.stop();
    });

    it("incorporated desires never repeat and later core migration stays authoritative", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "proven desktop renamed to its native one-based position");
        // The core publishes the bind: the spent desire never re-creates.
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: native.id, scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        assert.equal(h.world.created.length, 1, "spent desire must not re-create");
        showNative(h, "out-secondary", (h.world.desktops.find((d) => d.id === h.world.created[0]) as FakeDesktop).id);
        dispatchObserve(h);
        // The core bind incorporates the spent receipt; the adapter reports
        // native order/current with no logical views lane.
        assertNoViews(h);
        const acked = nativeOf(h).find((b) => b.out === "out-secondary");
        assert.ok(acked !== undefined);
        assert.ok(acked.order.includes(h.world.created[0] as string));
        assert.equal(acked.current, h.world.created[0]);
        assert.equal(attemptsOf(h).length, 0, "incorporated receipt retires");
        // Later legitimate migration of the same native to a new output wins.
        const r4 = JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r4.source, r4.nativeInvalid, "4", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-primary", bind: native.id, scope: { kind: "output", out: "out-primary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        showNative(h, "out-primary", native.id);
        dispatchObserve(h);
        // The migrated native rides the target output's order/current; no
        // logical views lane rides, and nothing re-creates.
        assertNoViews(h);
        const migrated = nativeOf(h).find((b) => b.out === "out-primary");
        assert.ok(migrated !== undefined);
        assert.ok(migrated.order.includes(native.id));
        assert.equal(migrated.current, native.id, "core migration wins");
        assert.equal(h.world.created.length, 1, "migration must not re-create");
        h.handle?.stop();
    });

    it("throwing create records a threw receipt with no binding until the core binds", () => {
        const h = makeTwoOutput({ throwOnce: true });
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        // Throw left the native uncreated; nothing bound yet.
        assert.equal(h.world.created.length, 0);
        // A later desktop is core-side evidence only: the spent op never
        // claims it and never fires a second native call.
        h.world.desktops.push(desk("native-9", "wl-1"));
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        assert.equal(h.world.created.length, 0, "spent throwing op must not append");
        const receipts = attemptsOf(h);
        assert.equal(receipts[0]?.["setter"], "threw", "threw receipt preserved");
        assert.equal(receipts[0]?.["created"], null, "no same-turn delta claims no id");
        // Only the core bind publishes the view: show the desktop, dispatch
        // a fresh capture, then answer it with the core-published bind.
        showNative(h, "out-secondary", "native-9");
        const r3 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r3.source, r3.nativeInvalid, "3", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "native-9", scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        dispatchObserve(h);
        // The core bind incorporates the threw receipt; the adapter reports
        // native order/current with no logical views lane.
        assertNoViews(h);
        const row = nativeOf(h).find((b) => b.out === "out-secondary");
        assert.ok(row !== undefined);
        assert.ok(row.order.includes("native-9"), `created native rides the order, got ${JSON.stringify(nativeOf(h))}`);
        assert.equal(row.current, "native-9");
        assert.equal(attemptsOf(h).length, 0, "core bind incorporates the receipt");
        h.handle?.stop();
    });

    it("two desires on two outputs execute independently with no shared native", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wl-1", out: "out-secondary", bind: "", scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
                { id: "wl-2", out: "out-primary", bind: "", scope: { kind: "output", out: "out-primary" }, order: 3, mode: "tiled", exist: true, hold: "" },
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary"), DESIRE("op-native-2", "wl-2", "out-primary")],
        }));
        assert.equal(h.world.created.length, 2);
        const n1 = h.world.desktops.find((d) => d.id === h.world.created[0]);
        const n2 = h.world.desktops.find((d) => d.id === h.world.created[1]);
        assert.ok(n1 !== undefined && n2 !== undefined);
        assert.notEqual(n1.id, n2.id, "no duplicate native binding");
        assert.equal(n1.name, "Desktop 3");
        assert.equal(n2.name, "Desktop 4", "second token uses its own enumerated position");
        // Before any core bind, the adapter publishes native order/current
        // with no logical views lane: both receipts ride causally instead.
        showNative(h, "out-secondary", n1.id);
        showNative(h, "out-primary", n2.id);
        dispatchObserve(h);
        assertNoViews(h);
        const rows = nativeOf(h);
        assert.ok(rows.find((b) => b.out === "out-secondary")?.order.includes(n1.id), "created native rides its output order");
        assert.ok(rows.find((b) => b.out === "out-primary")?.order.includes(n2.id), "created native rides its output order");
        assert.equal(attemptsOf(h).length, 2, "both receipts preserved");
        // Ack only wl-1: the wl-2 receipt must survive (no map-wide clear).
        const ack = JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", ack.source, ack.nativeInvalid, "2", {
            workspaces: [
                { id: "wl-1", out: "out-secondary", bind: n1.id, scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
                { id: "wl-2", out: "out-primary", bind: "", scope: { kind: "output", out: "out-primary" }, order: 3, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        dispatchObserve(h);
        // Only the wl-1 receipt incorporates; no logical views lane rides
        // and the wl-2 receipt survives (no map-wide clear).
        assertNoViews(h);
        const after = nativeOf(h);
        assert.ok(after.find((b) => b.out === "out-secondary")?.order.includes(n1.id));
        assert.equal(attemptsOf(h).length, 1, "only the wl-2 receipt survives");
        assert.equal(h.world.created.length, 2);
        // The wl-2 desire stays spent: still no second native call.
        const ack2 = JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", ack2.source, ack2.nativeInvalid, "3", {
            workspaces: [
                { id: "wl-1", out: "out-secondary", bind: n1.id, scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
                { id: "wl-2", out: "out-primary", bind: "", scope: { kind: "output", out: "out-primary" }, order: 3, mode: "tiled", exist: true, hold: "" },
            ],
            nativeCreations: [DESIRE("op-native-2", "wl-2", "out-primary")],
        }));
        assert.equal(h.world.created.length, 2, "retained desire never re-executes");
        h.handle?.stop();
    });
});
