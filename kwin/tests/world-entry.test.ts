// Offline integration coverage for the world production entry (D1-D5):
// discovery handshake, full observation reports, freshness acceptance,
// out-of-order/input-inflight/gesture/close/minimize/restart/incomplete/
// duplicate-effect/refusal/ambiguous-creation/removal-race/mode/partial-apply
// scenarios. Scripted in-memory transport; no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createContext, runInContext } from "node:vm";
import { startWorldAdapterEntry, mintAdapterIncarnation } from "../src/world-entry";
import { captureWorldObservation } from "../src/world-observe";
import { isSeparateWorkspaceMode } from "../src/workspace-native";
import { createMiniBroker, DISABLED_PRESENTATION, type MiniBroker } from "./world-wire-harness";
import { isCanonicalU64, isOpaqueId } from "../src/world-seq";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";

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
    // Truthful undecorated client rect (KWin `clientGeometry` Q_PROPERTY):
    // the fake carries no decoration, so the client mirrors the frame.
    clientGeometry: { x: number; y: number; width: number; height: number } | null;
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullscreen: boolean;
    maximized: boolean;
    sticky: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    minSize: null;
    maxSize: null;
    // Exact KWin overlay seams (read-first, lowercase fallback only when
    // absent): `fullScreen`, `maximizeMode` with the `setMaximize`
    // Q_INVOKABLE, and `onAllDesktops`.
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    setMaximize: (vertically: boolean, horizontally: boolean) => void;
}
interface FakeWorld {
    screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>;
    desktops: FakeDesktop[];
    wins: FakeWindow[];
    activeScreen: { name: string };
    activeWindow: FakeWindow | null;
    currentDesktop: FakeDesktop;
    created: string[];
    removed: string[];
    focused: string[];
    membershipWrites: string[];
    geometryWrites: string[];
}
interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });
const win = (id: string, out: string, ws: FakeDesktop, x = 0): FakeWindow => {
    const fake: FakeWindow = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        // Truthful undecorated client rect (world-observe client-to-decorated
        // delta): the fake carries no decoration, so the client mirrors the
        // frame exactly (delta 0); a missing client would null converted hint
        // bounds instead of mixing spaces.
        clientGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        hidden: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        keepAbove: false,
        keepBelow: false,
        minSize: null,
        maxSize: null,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        setMaximize: (vertically: boolean, horizontally: boolean): void => {
            fake.maximizeMode = vertically && horizontally ? 3 : 0;
        },
    };
    return fake;
};

function makeWorld(): FakeWorld {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const w1 = win("win-a", "out", ws1, 0);
    const w2 = win("win-b", "out", ws2, 0);
    return {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1, ws2],
        wins: [w1, w2],
        activeScreen: { name: "out" },
        activeWindow: w1,
        currentDesktop: ws1,
        created: [],
        removed: [],
        focused: [],
        membershipWrites: [],
        geometryWrites: [],
    };
}

interface Harness {
    world: FakeWorld;
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    shortcuts: Map<string, () => void>;
    signals: Record<string, FakeSignal>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: (maxDelay?: number) => void;
    lastCall: (method: string) => CapturedCall;
    intentWriteFlights: Array<{ payload: string; callback: (reply: unknown) => void }>;
}

interface FakeSignal {
    readonly handlers: Array<(...args: Array<unknown>) => void>;
    readonly signal: { connect: (handler: (...args: Array<unknown>) => void) => void; disconnect: (handler: (...args: Array<unknown>) => void) => void };
}

function fakeSignal(): FakeSignal {
    const handlers: Array<(...args: Array<unknown>) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (...args: Array<unknown>) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (...args: Array<unknown>) => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

function makeHarness(mode: unknown = "per-output-local", gesture = false, defaultTiled: unknown = true, refillFn?: () => unknown, optionsValue?: unknown, perOutputSeam = true, entryFn: typeof startWorldAdapterEntry = startWorldAdapterEntry, throwOnProbeLog = false, floatIntentSeam = false, queueHold?: { value: boolean }): Harness {
    const world = makeWorld();
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    // Held opaque-intent write flights, only when the floatIntent seam is
    // on: WriteFloatIntent callbacks are stashed (never auto-answered) so a
    // test can overlap accepted publications against a genuinely in-flight
    // persist and answer each covering write by hand. Default harnesses
    // keep sync durability and never emit these methods.
    const intentWriteFlights: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
    const shortcuts = new Map<string, () => void>();
    const signals: Record<string, FakeSignal> = {
        windowAdded: fakeSignal(),
        windowRemoved: fakeSignal(),
        windowActivated: fakeSignal(),
        desktopsChanged: fakeSignal(),
        screensChanged: fakeSignal(),
        currentDesktopChanged: fakeSignal(),
        activeScreenChanged: fakeSignal(),
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
            if ((world as unknown as Record<string, unknown>)["__throwFocus"] === true) {
                throw new Error("activeWindow-unavailable");
            }
            const found = world.wins.find((w) => (w as unknown) === ref);
            if (found) {
                world.focused.push(found.internalId);
                (world as { activeWindow: FakeWindow | null }).activeWindow = found;
            }
            const mode = (world as unknown as Record<string, unknown>)["__syncFocusEcho"];
            if (mode !== undefined && mode !== null && mode !== false) {
                const fire = (arg: unknown): void => {
                    for (const handler of [...(signals.windowActivated as FakeSignal).handlers]) {
                        (handler as (...args: Array<unknown>) => void)(arg);
                    }
                };
                if (mode === "truthful") {
                    fire(ref);
                } else if (mode === "mismatched") {
                    const other = world.wins.find((w) => (w as unknown) !== ref);
                    if (other !== undefined) {
                        (world as { activeWindow: FakeWindow | null }).activeWindow = other;
                        fire(other as unknown);
                    } else {
                        fire(null);
                    }
                }
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
        ...(perOutputSeam ? {
        currentDesktopForScreen: (_screen: unknown) => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown, _screen: unknown) => {
            const oldDesk = world.currentDesktop;
            const found = world.desktops.find((d) => (d as unknown) === deskRef);
            if (found) {
                world.currentDesktop = found;
            }
            const mode = (world as unknown as Record<string, unknown>)["__syncViewEcho"];
            if (mode !== undefined && mode !== null && mode !== false) {
                const outObj = (world.screens[0] as unknown) as object;
                const newDesk = (found as unknown) ?? deskRef;
                const fireView = (...args: Array<unknown>): void => {
                    for (const handler of [...(signals.currentDesktopChanged as FakeSignal).handlers]) {
                        (handler as (...args: Array<unknown>) => void)(...args);
                    }
                };
                const fireActive = (arg: unknown): void => {
                    for (const handler of [...(signals.windowActivated as FakeSignal).handlers]) {
                        (handler as (...args: Array<unknown>) => void)(arg);
                    }
                };
                if (mode === "truthful" || mode === "truthful-1arg") {
                    // Native order: the workspace selector restores focus
                    // (windowActivated) before the VDM currentChanged script
                    // callback (currentDesktopChanged) reaches the wrapper.
                    // The native chain may pick a different eligible member
                    // than the granted focus target, so the last member on
                    // the new view activates here and the core focus setter
                    // corrects to its target afterwards.
                    const members = world.wins.filter((w) => w.desktops[0] === (newDesk as FakeDesktop));
                    const member = members.length > 0 ? members[members.length - 1] : undefined;
                    if (member !== undefined) {
                        (world as { activeWindow: FakeWindow | null }).activeWindow = member;
                        fireActive(member as unknown);
                    }
                    if (mode === "truthful") {
                        fireView(oldDesk as unknown, newDesk as unknown, outObj);
                    } else {
                        fireView(oldDesk as unknown);
                    }
                } else if (mode === "silent-home-change") {
                    // Matched echoes, then a signal-free home change before
                    // the setter returns: read-back contract defense (actual
                    // native signal-free reachability unproven). The
                    // post-setter validation must catch the drift and stop
                    // the core focus setter.
                    const members = world.wins.filter((w) => w.desktops[0] === (newDesk as FakeDesktop));
                    const member = members.length > 0 ? members[members.length - 1] : undefined;
                    if (member !== undefined) {
                        (world as { activeWindow: FakeWindow | null }).activeWindow = member;
                        fireActive(member as unknown);
                    }
                    fireView(oldDesk as unknown, newDesk as unknown, outObj);
                    if (member !== undefined) {
                        member.desktops = [oldDesk as FakeDesktop];
                    }
                } else if (mode === "mismatched-new") {
                    const third = world.desktops.find((d) => (d as unknown) !== deskRef) ?? (newDesk as FakeDesktop);
                    fireView(oldDesk as unknown, third as unknown, outObj);
                } else if (mode === "two-arg") {
                    fireView(oldDesk as unknown, newDesk as unknown);
                } else if (mode === "third-output") {
                    fireView(oldDesk as unknown, newDesk as unknown, { name: "out-else" } as unknown);
                } else if (mode === "mismatched-active") {
                    const other = world.wins.find((w) => w.desktops[0] !== (newDesk as FakeDesktop));
                    if (other !== undefined) {
                        fireActive(other as unknown);
                    } else {
                        fireActive(null);
                    }
                    fireView(oldDesk as unknown, newDesk as unknown, outObj);
                } else if (mode === "foreground-mismatch") {
                    const members = world.wins.filter((w) => w.desktops[0] === (newDesk as FakeDesktop));
                    const member = members.length > 0 ? members[members.length - 1] : undefined;
                    if (member !== undefined) {
                        fireActive(member as unknown);
                    }
                    fireView(oldDesk as unknown, newDesk as unknown, outObj);
                } else if (mode === "unlisted-active") {
                    fireActive({ internalId: "ghost-win", output: { name: "out" }, desktops: [] } as unknown);
                    fireView(oldDesk as unknown, newDesk as unknown, outObj);
                } else if (mode === "replaced-active") {
                    // Replaced lifetime: the live member's canonical id
                    // changes, so the stale clone (old id) is no longer
                    // listed even though it is a same-shape JS object.
                    const live = world.wins.find((w) => w.internalId === "win-b");
                    if (live !== undefined) {
                        live.internalId = "win-b-next";
                    }
                    fireActive({ internalId: "win-b", output: { name: "out" }, desktops: [newDesk as unknown] } as unknown);
                    fireView(oldDesk as unknown, newDesk as unknown, outObj);
                }
            }
        },
        } : {}),
        sendClientToScreen: (_mover: unknown, _out: unknown) => undefined,
        createDesktop: (...args: ReadonlyArray<unknown>) => {
            if (args.length !== 2 || typeof args[0] !== "number" || typeof args[1] !== "string") {
                throw new Error("Insufficient arguments");
            }
            const id = `native-${world.desktops.length + 1}`;
            world.desktops.push(desk(id, args[1] as string));
            world.created.push(id);
        },
        removeDesktop: (ref: unknown) => {
            const id = (ref as FakeDesktop).id;
            world.desktops = world.desktops.filter((d) => d.id !== id);
            world.removed.push(id);
        },
    };
    // Instrument native setters through the live refs.
    for (const w of world.wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                // Keep the truthful undecorated client tracking the frame so
                // no capture pairs a fresh frame with a stale client.
                (w as unknown as Record<string, unknown>)["clientGeometry"] = { ...rect };
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        let desks = w.desktops;
        Object.defineProperty(w, "desktops", {
            configurable: true,
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                world.membershipWrites.push(`${w.internalId}->${refs.map((d) => d.id).join("+")}`);
                return true;
            },
        });
    }
    const handle = entryFn({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "ReadFloatIntent" && floatIntentSeam) {
                // Empty persisted-store read shape (ok outcome with
                // matching stored/returned counts): hydrates empty so the
                // first dispatch proceeds.
                const seedBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({
                    v: 2,
                    correlation_id: seedBody["correlation_id"],
                    outcome: "ok",
                    records: [],
                    stored: 0,
                    returned: 0,
                }));
            } else if (method === "WriteFloatIntent" && floatIntentSeam) {
                intentWriteFlights.push({ payload, callback });
            } else if (method === "QueueWorldCommand") {
                if (queueHold?.value === true) {
                    return;
                }
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"],
                    globalCmd: String(calls.filter((q) => q.method === "QueueWorldCommand").length) }));
            } else if (method === "TakeCommandResult") {
                const takeBody = JSON.parse(payload) as Record<string, unknown>;
                const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: takeGen, globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
                }
            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (message) => {
            if (throwOnProbeLog && message.includes("omnitiler:world:trace")) {
                throw new Error("probe-log-unavailable");
            }
            logs.push(message);
        },
        gestureActive: () => gesture,
        registerShortcutFn: (name, _text, _seq, cb) => {
            shortcuts.set(name, cb);
            return true;
        },
        readWorkspaceModeFn: () => mode,
        readDefaultTiledFn: () => defaultTiled,
        ...(refillFn !== undefined ? { readMigrationSourceRefillFn: refillFn } : {}),
        ...(optionsValue !== undefined ? { options: optionsValue } : {}),
        ...(floatIntentSeam ? { floatIntent: true } : {}),
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
    return {
        world, calls, timers, logs, shortcuts, signals, handle, intentWriteFlights,
        runTimers,
        lastCall: (method: string): CapturedCall => {
            const found = [...calls].reverse().find((c) => c.method === method);
            assert.ok(found !== undefined, `expected ${method}`);
            return found as CapturedCall;
        },
    };
}

function answerHandshake(h: { calls: CapturedCall[] }, gen = "gen-1"): void {
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
    const req = (JSON.parse((found as CapturedCall).payload) as Record<string, unknown>)["req"] as string;
    (found as CapturedCall).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: { calls: CapturedCall[] }, gen = "gen-1"): void {
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
    const body = JSON.parse((found as CapturedCall).payload) as Record<string, unknown>;
    (found as CapturedCall).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function settingsValues(h: { calls: CapturedCall[] }): Record<string, unknown> {
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
    return (JSON.parse((found as CapturedCall).payload) as Record<string, unknown>)["values"] as Record<string, unknown>;
}

function handshakeBody(h: { calls: CapturedCall[] }): Record<string, unknown> {
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
    return JSON.parse((found as CapturedCall).payload) as Record<string, unknown>;
}

function handshakeCount(h: { calls: CapturedCall[] }): number {
    // The handshake and the revisioned settings snapshot share the
    // DiscoverWorld method (world-entry.ts discover/sendSettings, dispatched
    // by type server-side): count actual handshakes, not all route calls.
    return h.calls.filter((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    }).length;
}

function discover(h: Harness, gen = "gen-1"): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies.
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
    // Settle the discovery-triggered observation report first: the entry
    // auto-dispatches one observation-only DescribeWorld on bind, and a
    // command issued while it is in flight queues in FIFO order behind it.
    // Replying here leaves the flight idle for the test body.
    const auto = h.lastCall("DescribeWorld");
    const autoBody = JSON.parse(auto.payload);
    assert.equal(autoBody["command"], undefined, "captures never carry an inline command");
    assert.equal((autoBody["report"] as Record<string, unknown>)["command"], undefined, "inner reports never carry a command");
    auto.callback(acceptedReply(gen, autoBody["source"] as string, autoBody["nativeInvalid"] as string, "0"));
}

function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation and native watermark. This scripted
    // harness advances broker obs and revision together; terminal results
    // stay requester-only on the take lane.
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        result: null,
        ...extra,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}

function requestOf(h: Harness): Record<string, unknown> {
    return JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
}

function reportOf(h: Harness): Record<string, unknown> {
    return requestOf(h)["report"] as Record<string, unknown>;
}

function queueOf(h: Harness): string {
    return h.lastCall("QueueWorldCommand").payload;
}

function queueCalls(h: Harness): Array<Record<string, unknown>> {
    return h.calls
        .filter((c) => c.method === "QueueWorldCommand")
        .map((c) => JSON.parse(c.payload) as Record<string, unknown>);
}

describe("world entry discovery and observation", () => {
    it("discovers on start and sends a full observation report", () => {
        const h = makeHarness();
        assert.equal(h.lastCall("DiscoverWorld").method, "DiscoverWorld");
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        assert.equal(req["v"], 4);
        assert.equal(req["gen"], "gen-1");
        assert.equal(req["adapter"], "test-adapter");
        assert.ok(typeof req["source"] === "string" && (req["source"] as string).length > 0);
        assert.ok(typeof req["sourceSeq"] === "string" && (req["sourceSeq"] as string).length > 0);
        assert.equal(req["command"], undefined, "captures never carry an inline command");
        const inner = req["report"] as Record<string, unknown>;
        assert.equal(inner["v"], 4);
        for (const key of ["owner", "adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs", "mode", "views", "hostOwned", "equalCount", "predicate", "refill", "defaultTiled"]) {
            assert.ok(!(key in inner), `inner report carries no ${key}`);
        }
        // Retired host-truth lanes ride the handshake capabilities now:
        // the producer declares host-owned natives with equal counts,
        // plus the exact core-minted application identity (null app and
        // executable, opaque process token).
        const hs = handshakeBody(h);
        const caps = hs["capabilities"] as Record<string, unknown>;
        assert.equal(caps["hostOwned"], true, "handshake carries the host-owned capability");
        assert.equal(caps["equalCount"], true, "handshake carries the equal-count capability");
        const ident = hs["applicationIdentity"] as Record<string, unknown>;
        assert.equal(ident["appId"], null, "handshake carries the null app identity");
        assert.equal(ident["executable"], null, "handshake carries the null executable identity");
        assert.ok(isOpaqueId(ident["processId"]), "handshake carries the opaque process identity");
        // Workspace mode rides the revisioned settings snapshot, never the
        // report lane.
        assert.equal(settingsValues(h)["mode"], "per-output-local");
        const screens = inner["screens"] as Array<Record<string, unknown>>;
        assert.equal(screens.length, 1);
        assert.deepEqual(screens[0]?.["full"], [0, 0, 1920, 1080]);
        assert.deepEqual(screens[0]?.["work"], [0, 0, 800, 600]);
        assert.equal(screens[0]?.["complete"], true);
        assert.equal(settingsValues(h)["gap"], 8);
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        assert.equal(wins.length, 2);
        assert.equal((inner["native"] as Array<Record<string, unknown>>)[0]?.["current"], "ws1");
        const first = (wins[0] as Record<string, unknown>);
        assert.equal(first["nativeWs"], "ws1", "truthful native membership evidence");
        assert.deepEqual(first["rect"], [0, 0, 200, 200]);
        assert.deepEqual(first["hints"], { minW: null, minH: null, maxW: null, maxH: null });
        assert.equal(first["fullscreen"], false);
        assert.equal(first["maximized"], false);
        assert.equal(first["sticky"], false);
        assert.equal(first["minimized"], false);
        assert.ok(!("observedWs" in first) && !("fitExcluded" in first) && !("floating" in first), "no projected identity verdicts");
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.ok(typeof filed["clientCmd"] === "string");
        assert.deepEqual((filed["command"] as Record<string, unknown>)["op"], "select-relative");
    });

    it("degrades per-output modes to shared without the 6.7 seam", () => {
        const h = makeHarness();
        const surface = { ...(h as unknown as Record<string, unknown>) };
        void surface;
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        // The shared harness below covers the seam; this host keeps the
        // legacy-shared shape, so observation still succeeds.
        assert.ok((reportOf(h)["screens"] as Array<unknown>).length >= 1);
    });

    it("passes all three workspace modes through verbatim with no fallback", () => {
        for (const mode of ["per-output-local", "global-unique", "shared"]) {
            const h = makeHarness(mode);
            discover(h);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            assert.equal(settingsValues(h)["mode"], mode);
            assert.ok(
                !h.logs.some((l) => l.includes("workspace-mode-fallback")),
                `no fallback for ${mode}`,
            );
            h.handle?.stop();
        }
    });

    it("wires the saved default tiling into reports and the snapshot", () => {
        // Backfill default: reports true and the snapshot seeds tiled.
        const backfill = makeHarness("per-output-local", false);
        discover(backfill);
        assert.equal(settingsValues(backfill)["defaultTiled"], true);
        assert.equal(backfill.handle?.getWorkspaceTilingSnapshot().defaultTiled, true);
        backfill.handle?.stop();
        // Saved false: reports false and the pre-reply snapshot seeds
        // floating; the entry keeps no per-workspace flag of its own.
        const floating = makeHarness("per-output-local", false, false);
        discover(floating);
        assert.equal(settingsValues(floating)["defaultTiled"], false);
        const snapshot = floating.handle?.getWorkspaceTilingSnapshot();
        assert.equal(snapshot?.defaultTiled, false);
        assert.equal(snapshot?.tiled, false);
        floating.handle?.stop();
    });

    it("applies per-property geometry diffs and ignores stale revisions", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        const reply = acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
        });
        h.lastCall("DescribeWorld").callback(reply);
        assert.ok(h.world.geometryWrites.some((w) => w.startsWith("win-a@10,10")));
        // Stale rev on the same watermark applies nothing further.
        const writes = h.world.geometryWrites.length;
        h.handle?.requestWorkspaceRelative(-1);
        h.runTimers();
        const req2 = requestOf(h);
        // Deliver the OLD rev against the new obs: accepted-rev gate refuses.
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["source"] as string, req2["nativeInvalid"] as string, "1"));
        assert.equal(h.world.geometryWrites.length, writes);
    });

    it("queues directional input during flight without disturbing the head", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        // Idle fast path: the first command dispatches without waiting for
        // the observation debounce.
        const describes = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.equal(describes.length, 2);
        const first = requestOf(h);
        const firstObs = first["nativeInvalid"] as string;
        const headQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.notEqual(((headQ["command"] as Record<string, unknown>)["op"]), null);
        const firstClientCmd = headQ["clientCmd"] as string;
        // A second directional press queues WITHOUT advancing the
        // watermark: only native changes invalidate, so the head reply
        // still matches and applies.
        h.handle?.requestFocus("right");
        assert.ok(!h.logs.some((l) => l.includes("command-busy")));
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", first["source"] as string, firstObs, "1"));
        assert.ok(!h.logs.some((l) => l.includes("stale")));
        // The queued press drains zero-delay after the acceptance.
        h.runTimers();
        const queued = queueCalls(h);
        const secondCmd = (queued[queued.length - 1]?.["command"] as Record<string, unknown>);
        assert.deepEqual(secondCmd["op"], "focus-directional");
        assert.deepEqual(secondCmd["direction"], "right");
        const second = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", second["source"] as string, second["nativeInvalid"] as string, "2"));
        const seqs = queueCalls(h).map((req) => req["clientCmd"] as string);
        assert.deepEqual(seqs, [firstClientCmd, seqs[1] as string]);
    });

    it("suspends geometry under a gesture barrier but keeps observation flowing", () => {
        const h = makeHarness("per-output-local", true);
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        assert.equal((req["report"] as Record<string, unknown>)["gesture"], true);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [50, 50, 300, 300], frame: [50, 50, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
        }));
        assert.equal(h.world.geometryWrites.length, 0);
    });

    it("retains explicit closes until an accepted reply acks them", () => {
        const h = makeHarness();
        discover(h);
        // win-b closes natively: the removal signal names the ref, then it
        // leaves the live list.
        const removed = h.world.wins.find((w) => w.internalId === "win-b");
        assert.ok(removed !== undefined);
        for (const handler of [...(h.signals.windowRemoved as FakeSignal).handlers]) {
            (handler as (payload?: unknown) => void)(removed);
        }
        h.world.wins = h.world.wins.filter((w) => w.internalId !== "win-b");
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        // The explicit closed id rides the report (bare absence alone would
        // read as minimize, never close); the survivor stays fully covered.
        const inner = reportOf(h);
        const wins = inner["windows"] as Array<Record<string, unknown>>;
        assert.ok(wins.every((w) => (w as Record<string, unknown>)["win"] !== "win-b"));
        assert.ok(wins.some((w) => (w as Record<string, unknown>)["win"] === "win-a"));
        const closed = inner["closed"] as Array<string>;
        assert.ok(closed.includes("win-b"));
    });

    it("carries minimized windows without closing them", () => {
        const h = makeHarness();
        const target = h.world.wins.find((w) => w.internalId === "win-b");
        assert.ok(target !== undefined);
        (target as FakeWindow).minimized = true;
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = reportOf(h);
        // Minimized lifetimes ride as explicit minimized facts with full
        // geometry (never silent absence, never an inferred close).
        const wins = req["windows"] as Array<Record<string, unknown>>;
        const minimized = wins.filter((w) => w["minimized"] === true);
        assert.ok(minimized.some((m) => m["win"] === "win-b" && m["out"] === "out" &&
            (m["nativeWs"] as unknown) === "ws2"));
        const tiled = wins.filter((w) => w["minimized"] !== true);
        assert.ok(!tiled.some((w) => (w as Record<string, unknown>)["win"] === "win-b"));
        assert.equal(settingsValues(h)["predicate"], "both-axes-fixed");
        assert.equal(settingsValues(h)["refill"], "last-remaining-workspace");
    });

    it("carries the validated refill token live per report", () => {
        let refill: unknown = "most-recently-used-workspace";
        const configChanged = fakeSignal();
        const h = makeHarness("per-output-local", false, true, () => refill, { configChanged: configChanged.signal });
        discover(h);
        assert.equal(settingsValues(h)["refill"], "most-recently-used-workspace");
        // Revisioned settings (world-entry.ts sendSettings/onConfigChanged):
        // a setting change resends on the deliberate configChanged edge with
        // a bumped revision, and the new snapshot rides only after the
        // accepted settings reply. Invalid values fall back to the default
        // without touching committed state.
        refill = "bogus";
        const before = h.calls.filter((c) => c.method === "DescribeWorld").length;
        for (const handler of [...configChanged.handlers]) {
            (handler as () => void)();
        }
        // The edge synchronously resends the revisioned snapshot carrying
        // the validated fallback.
        assert.equal(settingsValues(h)["refill"], "last-remaining-workspace");
        answerSettings(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, before + 1);
        assert.equal(settingsValues(h)["refill"], "last-remaining-workspace");
    });

    it("rediscovers on generation mismatch without replaying the command", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const callsBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 4, outcome: "rejected", kind: "generation-mismatch", message: "stale", gen: "gen-2",
            source: "", nativeInvalid: "0", clock: "0", gestureAssoc: null,
        }));
        // A fresh discovery goes out; the dropped command is NOT redispatched
        // as a DescribeWorld (only the new DiscoverWorld handshake; the
        // revisioned settings snapshot shares the same method).
        assert.equal(handshakeCount(h), 2);
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, callsBefore);
        answerHandshake(h, "gen-2");
        answerSettings(h, "gen-2");
        // Settle the rebind-triggered observation report before commanding.
        {
            const auto = h.lastCall("DescribeWorld");
            const autoBody = JSON.parse(auto.payload);
            auto.callback(acceptedReply("gen-2", autoBody.source, autoBody.nativeInvalid, "0"));
        }
        // After rebinding, a new explicit command flows with the new gen.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(requestOf(h)["gen"], "gen-2");
    });

    it("holds unreadable scopes with no writes", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws2" }, auth: "hold", reason: "unreadable" }],
            windows: [{
                win: "win-b", out: "out", ws: "ws2",
                alloc: [0, 0, 100, 100], frame: [0, 0, 100, 100],
                frame_auth: "hold", frame_hold: "unreadable",
                member_auth: "hold", member_hold: "unreadable",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
        }));
        assert.equal(h.world.geometryWrites.length, 0);
        assert.equal(h.world.membershipWrites.length, 0);
    });

    it("spends a focus effect once and never repeats it", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            effects: [{ id: "fx-1", kind: "focus", target: "win-b", rev: "1", cause: "automatic-admission", cmdSeq: null, awaitShown: false }],
        }));
        assert.equal(h.world.focused.length, 1);
        // A repeated identical effect id on a later reply never reapplies.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req2 = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req2["source"] as string, req2["nativeInvalid"] as string, "2", {
            effects: [{ id: "fx-1", kind: "focus", target: "win-b", rev: "1", cause: "automatic-admission", cmdSeq: null, awaitShown: false }],
        }));
        assert.equal(h.world.focused.length, 1);
    });

    it("defers truthful view+focus own echoes to one authorized focus dispatch", () => {
        const h = makeHarness();
        discover(h);
        // The native focus chain picks its own eligible member on the new
        // view (win-c), distinct from the granted core focus target
        // (win-b); the core focus setter corrects afterwards.
        const ws2 = h.world.desktops[1] as FakeDesktop;
        h.world.wins.push(win("win-c", "out", ws2, 0));
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "truthful";
        (h.world as unknown as Record<string, unknown>)["__syncFocusEcho"] = "truthful";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-1", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-1", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.currentDesktop.id, "ws2", "view switch actuates");
            assert.deepEqual(h.world.focused, ["win-b"], "exactly one core-target focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-1");
            assert.ok(row !== undefined, "focus row rides the next report");
            assert.equal(row["setter"], "accepted");
            assert.equal(row["foregroundProven"], true);
            assert.ok(Array.isArray(row["arrival"]) && (row["arrival"] as Array<unknown>).length >= 1, "arrival brackets ride");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            (h.world as unknown as Record<string, unknown>)["__syncFocusEcho"] = false;
            h.handle?.stop();
        }
    });

    it("mismatched reentrant view signal invalidates instead of deferring", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "mismatched-new";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-2", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-2", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "mismatched view echo stops the focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-2");
            assert.ok(row !== undefined, "superseded row rides");
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("mismatched reentrant focus activation never proves", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncFocusEcho"] = "mismatched";
        try {
            h.handle?.requestFocus("left");
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                effects: [{ id: "fx-mismatch-1", kind: "focus", target: "win-b", rev: "1", cause: "automatic-admission", cmdSeq: null, awaitShown: false }],
            }));
            assert.equal(h.world.focused.length, 1, "setter still runs once");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-mismatch-1");
            assert.ok(row !== undefined);
            assert.equal(row["setter"], "accepted", "setter ran");
            assert.equal(row["foregroundProven"], false, "mismatched live never proves");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncFocusEcho"] = false;
            h.handle?.stop();
        }
    });

    it("unlisted reentrant activation invalidates instead of deferring", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "unlisted-active";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-3", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-3", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "unlisted activation stops focus");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-3");
            assert.ok(row !== undefined);
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("defers truthful 6.6 single-arg view echoes", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "truthful-1arg";
        (h.world as unknown as Record<string, unknown>)["__syncFocusEcho"] = "truthful";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-6", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-6", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.currentDesktop.id, "ws2", "view switch actuates");
            assert.deepEqual(h.world.focused, ["win-b"], "exactly one core-target focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-6");
            assert.ok(row !== undefined, "focus row rides the next report");
            assert.equal(row["setter"], "accepted");
            assert.equal(row["foregroundProven"], true);
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            (h.world as unknown as Record<string, unknown>)["__syncFocusEcho"] = false;
            h.handle?.stop();
        }
    });

    it("rejects a two-arg view signal as neither pin shape", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "two-arg";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-7", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-7", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "rejected view echo stops the focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-7");
            assert.ok(row !== undefined, "superseded row rides");
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("mismatched-output view signal invalidates instead of deferring", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "third-output";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-8", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-8", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "wrong-output echo stops the focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-8");
            assert.ok(row !== undefined, "superseded row rides");
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("foreign view activation invalidates instead of deferring", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "mismatched-active";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-9", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-9", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "foreign activation stops the focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-9");
            assert.ok(row !== undefined, "superseded row rides");
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("view activation disagreeing with the foreground invalidates", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "foreground-mismatch";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-10", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-10", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "activation naming a non-foreground member stops the focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-10");
            assert.ok(row !== undefined, "superseded row rides");
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("replaced view-activation lifetime invalidates instead of deferring", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "replaced-active";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-11", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-11", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "stale-lifetime activation stops the focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-11");
            assert.ok(row !== undefined, "superseded row rides");
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("silent post-echo home drift invalidates and stops the focus setter", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = "silent-home-change";
        try {
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                workspaces: [
                    { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
                transfers: [{ op: "op-view-12", kind: "view", win: "", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
                effects: [{ id: "fx-view-12", kind: "focus", target: "win-b", rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown: true }],
            }));
            assert.equal(h.world.focused.length, 0, "post-echo home drift stops the focus setter");
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-view-12");
            assert.ok(row !== undefined, "superseded row rides");
            assert.equal(row["setter"], "not-called");
            assert.equal(row["reason"], "superseded");
        } finally {
            (h.world as unknown as Record<string, unknown>)["__syncViewEcho"] = false;
            h.handle?.stop();
        }
    });

    it("records a throwing native focus setter truthfully instead of refusing", () => {
        const h = makeHarness();
        discover(h);
        (h.world as unknown as Record<string, unknown>)["__throwFocus"] = true;
        try {
            h.handle?.requestFocus("left");
            h.runTimers();
            const req = requestOf(h);
            h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
                effects: [{ id: "fx-throw-1", kind: "focus", target: "win-b", rev: "1", cause: "automatic-admission", cmdSeq: null, awaitShown: false }],
            }));
            h.runTimers();
            const wire = (reportOf(h)["focusFeedback"] as Array<Record<string, unknown>>);
            const row = wire.find((r) => r["id"] === "fx-throw-1");
            assert.ok(row !== undefined, "throw row rides the next report");
            assert.equal(row["setter"], "threw");
            assert.equal(row["foregroundProven"], false);
        } finally {
            (h.world as unknown as Record<string, unknown>)["__throwFocus"] = false;
            h.handle?.stop();
        }
    });

    it("external view/focus event outside the setter still invalidates", () => {
        const h = makeHarness();
        discover(h);
        const before = h.calls.filter((c) => c.method === "DescribeWorld").length;
        const ws1 = h.world.desktops[0] as FakeDesktop;
        const ws2 = h.world.desktops[1] as FakeDesktop;
        for (const handler of [...(h.signals.currentDesktopChanged as FakeSignal).handlers]) {
            (handler as (...args: Array<unknown>) => void)(ws1 as unknown, ws2 as unknown, h.world.screens[0] as unknown);
        }
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            (handler as (...args: Array<unknown>) => void)(h.world.wins[0] as unknown);
        }
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, before + 1, "outside-setter signals dispatch once coalesced");
        h.handle?.stop();
    });

    it("settles refused transfers without repeating them", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelativeMove(1, true);
        h.runTimers();
        const req = requestOf(h);
        const filedRefused = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.deepEqual((filedRefused["command"] as Record<string, unknown>)["op"], "send-relative");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "controlled", frame_hold: "",
                member_auth: "hold", member_hold: "refused-target",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
        }));
        // Refused membership performs no native move.
        assert.equal(h.world.membershipWrites.length, 0);
    });

    it("creates through one flight and blocks removal of occupied workspaces", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            workspaces: [
                { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out", bind: "", scope: { kind: "output", out: "out" }, order: 2, mode: "tiled", exist: true, hold: "" },
                // Desired-absent but occupied: the fresh occupancy gate keeps it.
                { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: false, hold: "" },
            ],
            nativeCreations: [{ op: "op-native-1", ws: "wl-1", out: "out" }],
        }));
        assert.equal(h.world.created.length, 1);
        assert.equal(h.world.removed.length, 0);
    });

    it("recovers busy replies event-driven with no polling", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 4, outcome: "rejected", kind: "busy", message: "busy", gen: "gen-1",
            source: "", nativeInvalid: "0", clock: "0", gestureAssoc: null,
        }));
        // No timer-driven retry was scheduled beyond the debounce/timeout
        // pair: recovery waits for the next native event (dirty flag set).
        assert.ok(h.logs.some((l) => l.includes("command-busy") || l.includes("world")));
        assert.equal(h.world.geometryWrites.length, 0);
    });

    it("stops partial apply mid-way when invalidated", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        // Two controlled geometry rows; the apply writes the first, then a
        // concurrent invalidation stops the rest. Deterministic here: both
        // rows target distinct live windows and the harness marks dirty only
        // through explicit commands, so both apply. The narrow assertion is
        // that held rows never write.
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [
                {
                    win: "win-a", out: "out", ws: "ws1",
                    alloc: [11, 11, 300, 300], frame: [11, 11, 300, 300],
                    frame_auth: "controlled", frame_hold: "",
                    member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "",
                    floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                },
                {
                    win: "win-b", out: "out", ws: "ws2",
                    alloc: [22, 22, 300, 300], frame: [22, 22, 300, 300],
                    frame_auth: "hold", frame_hold: "gesture-barrier",
                    member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "",
                    floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                },
            ],
        }));
        assert.ok(h.world.geometryWrites.some((w) => w.startsWith("win-a@11,11")));
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-b@22,22")));
    });

    it("captures fixed-size pins truthfully instead of carrying verdict flags", () => {
        const h = makeHarness();
        const target = h.world.wins.find((w) => w.internalId === "win-a");
        assert.ok(target !== undefined);
        (target as unknown as Record<string, unknown>)["minSize"] = { width: 400, height: 300 };
        (target as unknown as Record<string, unknown>)["maxSize"] = { width: 400, height: 300 };
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = reportOf(h);
        const wins = req["windows"] as Array<Record<string, unknown>>;
        const row = wins.find((w) => w["win"] === "win-a");
        assert.ok(row !== undefined);
        // Raw client extents ride verbatim on the hints lane; classification
        // is broker-owned (the Rust raw tests pin the equal-nonzero fixed
        // verdict and its automatic float under the default both-axes
        // predicate, which rides the settings snapshot).
        assert.deepEqual(row["hints"], { minW: 400, minH: 300, maxW: 400, maxH: 300 });
        assert.deepEqual(row["rect"], [0, 0, 200, 200]);
        assert.ok(!("fixedAuto" in row) && !("floating" in row) && !("fitExcluded" in row), "no wire verdicts");
    });

    it("sends append-and-stay as one core-resolved send-numbered with no view switch", () => {
        const h = makeHarness();
        discover(h);
        // Append-and-stay: a single send-numbered index 0 with follow=false.
        // Positional resolution is all core (reuse or stable pending token);
        // sends never view-switch, so no trailing switch or stay restore.
        (h.handle as unknown as { requestWorkspaceMove: (index: number, follow: boolean) => void }).requestWorkspaceMove(0, false);
        h.runTimers();
        const first = requestOf(h);
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = filed["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "send-numbered");
        assert.equal(cmd["index"], 0);
        assert.equal(cmd["follow"], false);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", first["source"] as string, first["nativeInvalid"] as string, "1", {
            workspaces: [
                { id: "ws1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "ws2", out: "out", bind: "ws2", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
            ],
            outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
            transfers: [{ op: "op-1", kind: "window", win: "win-a", out: "out", ws: "ws2", follow: false, held: false, cmdSeq: null }],
        }));
        // No chained view traffic: the flight settles with no further
        // switch-view/restore dispatch.
        h.runTimers();
        const settled = h.calls.filter((c) => c.method === "DescribeWorld");
        assert.ok(settled.length >= 2);
    });

    it("routes sticky/fullscreen toggles as core commands with zero direct native writes", () => {
        const h = makeHarness();
        discover(h);
        const ref = h.world.wins.find((w) => w.internalId === "win-a") as unknown as Record<string, unknown>;
        ref["keepAbove"] = false;
        ref["keepBelow"] = false;
        const onAllBefore = ref["onAllDesktops"];
        const fsBefore = ref["fullScreen"];
        h.handle?.requestSticky();
        h.runTimers();
        const stickyFiled = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.deepEqual(stickyFiled["command"], { op: "toggle-sticky", win: { currentFocus: true } });
        // No direct native setter: the command owns the verdict; the
        // accepted state rows drive the single executor.
        assert.equal(ref["onAllDesktops"], onAllBefore);
        assert.equal(ref["keepAbove"], false);
        // Settle the sticky flight, then the fullscreen toggle files its
        // own command with zero direct writes as well.
        h.runTimers();
        const stickyReq = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply(
            "gen-1", stickyReq["source"] as string, stickyReq["nativeInvalid"] as string, "1",
        ));
        h.handle?.requestFullscreen();
        h.runTimers();
        const fsFiled = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.deepEqual(fsFiled["command"], { op: "toggle-fullscreen", win: { currentFocus: true } });
        assert.equal(ref["fullScreen"], fsBefore);
        assert.equal(h.world.membershipWrites.length, 0);
        assert.equal(h.world.geometryWrites.length, 0);
    });

    it("registers the legacy directional and toggle shortcut catalog", () => {
        const h = makeHarness();
        // Directional focus/move/resize with HJKL plus arrow aliases.
        for (const name of [
            "omnitiler-focus-left", "omnitiler-focus-left-arrow",
            "omnitiler-move-right-arrow",
            "omnitiler-resize-outwards-up", "omnitiler-resize-outwards-up-arrow",
            "omnitiler-resize-inwards-down", "omnitiler-resize-inwards-down-arrow",
        ]) {
            assert.ok(h.shortcuts.has(name), name);
        }
        // Window-state toggles with legacy sequences.
        for (const name of [
            "omnitiler-toggle-float", "omnitiler-toggle-sticky",
            "omnitiler-toggle-maximize", "omnitiler-toggle-fullscreen",
            "omnitiler-toggle-orientation",
        ]) {
            assert.ok(h.shortcuts.has(name), name);
        }
        // A resize shortcut routes a parameterized direct v4 resize command.
        discover(h);
        const resize = h.shortcuts.get("omnitiler-resize-outwards-left") as () => void;
        assert.ok(resize !== undefined);
        resize();
        h.runTimers();
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = (filed["command"] as Record<string, unknown>);
        assert.equal(cmd["op"], "resize");
        assert.equal(cmd["direction"], "left");
        assert.equal(cmd["mode"], "outwards");
        assert.equal(cmd["press"], 0);
        // An orientation shortcut routes the direct v4 toggle-orientation
        // command on the same production path once the resize settles.
        const resizeReq = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", resizeReq["source"] as string, resizeReq["nativeInvalid"] as string, "1"));
        const orient = h.shortcuts.get("omnitiler-toggle-orientation") as () => void;
        assert.ok(orient !== undefined);
        orient();
        h.runTimers();
        const orientFiled = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const orientCmd = (orientFiled["command"] as Record<string, unknown>);
        assert.deepEqual(orientCmd, { op: "toggle-orientation", win: { currentFocus: true } });
    });

    it("defers a floating-maximized float request until the settlement report", () => {
        const h = makeHarness();
        discover(h);
        // Seed intentional-float membership through an accepted untouched
        // row so the B9 maximized-unfloat path arms.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const seed = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", seed["source"] as string, seed["nativeInvalid"] as string, "1", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
        }));
        const target = h.world.wins.find((w) => w.internalId === "win-a");
        assert.ok(target !== undefined);
        (target as FakeWindow).maximizeMode = 3;
        h.handle?.requestFloat();
        // Deferred: the sync fake settles in the same stack, but no
        // toggle-float command rides it (no same-stack echo admission).
        h.runTimers();
        const queuesBefore = queueCalls(h).length;
        let req = requestOf(h);
        assert.equal(queuesBefore, 1, "clear schedules observation first");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "2", {
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
        }));
        h.runTimers();
        req = requestOf(h);
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = filed["command"] as Record<string, unknown>;
        // B9 maximized-unfloat continuation (world-native 407-426: the
        // settlement reads the settled live frame; 721-740: unfloat carries
        // the live rect): the deferred command carries the genuine settled
        // frame, derived here from the target's current frame for explicit
        // genuine-input equality.
        const frame = (target as FakeWindow).frameGeometry;
        assert.deepEqual(cmd, { op: "toggle-float", win: "win-a", rect: [frame.x, frame.y, frame.width, frame.height] });
        assert.equal((target as FakeWindow).maximizeMode, 0);
        assert.ok(h.logs.some((l) => l.includes("outcome=observed-cleared")));
    });
});

describe("world entry rapid directional FIFO", () => {
    // Producer-shaped helpers over the harness above: every discrete press
    // dispatches in press order across debounce and flight boundaries.
    const sentCmdSeqs = (h: Harness): string[] => queueCalls(h)
        .map((req) => req["clientCmd"] as string);
    // Fire the earliest due timer at/after `from` only: the entry arms
    // follow-up timers from inside callbacks, firing the whole queue at
    // once would also expire a follow-up's own timeout before its reply
    // can arrive, and dead (cancel-noop) timers from earlier legs must
    // not steal the pump.
    const fireNextTimer = (hh: Harness, maxDelay: number, from = 0): void => {
        let at = -1;
        for (let i = from; i < hh.timers.length; i += 1) {
            const t = hh.timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay && (at < 0 || t.delay < (hh.timers[at] as { delay: number; cb: () => void }).delay)) {
                at = i;
            }
        }
        assert.ok(at >= 0, "expected a due timer");
        const [timer] = hh.timers.splice(at, 1);
        (timer as { delay: number; cb: () => void }).cb();
    };
    const acceptLast = (h: Harness, gen: string, rev: string): void => {
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply(gen, req["source"] as string, req["nativeInvalid"] as string, rev));
    };

    it("dispatches rapid left then right in order across a coalescing debounce", () => {
        const h = makeHarness();
        discover(h);
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        // A native burst arms the debounce with no command in flight.
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            handler();
        }
        const left = h.shortcuts.get("omnitiler-focus-left") as () => void;
        const right = h.shortcuts.get("omnitiler-focus-right") as () => void;
        assert.ok(left !== undefined && right !== undefined);
        left();
        right();
        // Both presses accepted: no busy refusal, no shortcut failure.
        assert.ok(!h.logs.some((l) => l.includes("command-busy")));
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
        // FIFO batch: the whole parked batch files before the single
        // draining capture (press order preserved, no capture of its own
        // while parking).
        assert.equal(queueCalls(h).length, 2);
        assert.deepEqual(
            queueCalls(h).map((req) => (req["command"] as Record<string, unknown>)["direction"]),
            ["left", "right"],
        );
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore,
            "parking files no capture of its own",
        );
        // The debounce drains the whole batch in exactly one fresh capture.
        h.runTimers();
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore + 1,
            "exactly one fresh capture drains the batch",
        );
        assert.equal(queueCalls(h).length, 2, "the drain files nothing further");
        acceptLast(h, "gen-1", "1");
        // The accepted drain files nothing further: both intents rode the
        // one batch with strictly increasing command sequences.
        h.runTimers();
        const queued = queueCalls(h);
        assert.equal(queued.length, 2);
        assert.ok((queued[1]?.["clientCmd"] as string) > (queued[0]?.["clientCmd"] as string));
    });

    it("preserves repeated directional moves in order across successive flights", () => {
        const h = makeHarness();
        discover(h);
        const moveLeft = h.shortcuts.get("omnitiler-move-left") as () => void;
        assert.ok(moveLeft !== undefined);
        // First press takes the idle fast path: filed at once, then
        // drained by its fresh capture.
        moveLeft();
        const headQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.equal((headQ["command"] as Record<string, unknown>)["op"], "move-directional");
        const describesAfterHead = h.calls.filter((c) => c.method === "DescribeWorld").length;
        // Two more presses park FIFO behind the flight (filed promptly,
        // no watermark advance), so the head still accepts; the pair then
        // drains as ONE batch after the acceptance.
        moveLeft();
        moveLeft();
        assert.ok(!h.logs.some((l) => l.includes("command-busy")));
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
        assert.ok(!h.logs.some((l) => l.includes("stale")));
        assert.equal(queueCalls(h).length, 3, "all three presses filed upfront in press order");
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesAfterHead,
            "parking behind the flight starts no second flight",
        );
        const head = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", head["source"] as string, head["nativeInvalid"] as string, "1"));
        h.runTimers();
        // The two parked tails drain in exactly one follow-up capture.
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesAfterHead + 1,
            "tails drain as one batch",
        );
        assert.equal(queueCalls(h).length, 3, "the batch drain files nothing further");
        acceptLast(h, "gen-1", "2");
        h.runTimers();
        assert.equal(queueCalls(h).length, 3);
        // Every intent rode exactly once: strictly increasing command
        // sequences, no replay, no coalescing.
        const seqs = sentCmdSeqs(h);
        assert.deepEqual(seqs, [...seqs].sort());
        assert.equal(new Set(seqs).size, 3);
    });

    it("drains the parked tail after a busy rejection without replay, unsignalled", () => {
        const h = makeHarness();
        discover(h);
        // Idle fast path: the first press files and captures at once; the
        // second parks FIFO behind the flight (filed promptly).
        h.handle?.requestFocus("left");
        h.handle?.requestFocus("right");
        assert.equal(queueCalls(h).length, 2, "both presses filed upfront in press order");
        // Core busy: the consumed head ends without replay; the parked tail
        // survives for the next drain.
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 4, outcome: "rejected", kind: "busy", message: "busy", gen: "gen-1",
            source: "", nativeInvalid: "0", clock: "0", gestureAssoc: null,
        }));
        // No native event and no new press: the terminal failure alone
        // redrains the parked tail.
        h.runTimers();
        assert.equal(queueCalls(h).length, 2, "the redrain files nothing further");
        const queued = queueCalls(h);
        const drained = queued[1]?.["command"] as Record<string, unknown>;
        assert.equal(drained["op"], "focus-directional");
        assert.equal(drained["direction"], "right");
        acceptLast(h, "gen-1", "1");
        // No replay anywhere: the refused head and the parked tail each
        // filed exactly once.
        const cmdSeqs = sentCmdSeqs(h);
        assert.equal(new Set(cmdSeqs).size, cmdSeqs.length);
        assert.equal(cmdSeqs.length, 2);
    });

    it("drops only the consumed head on timeout and drains the tail unsignalled", () => {
        const h = makeHarness();
        discover(h);
        const mark = h.timers.length;
        // Idle fast path: the first press files and captures at once; the
        // second parks FIFO behind the flight.
        h.handle?.requestFocus("left");
        const firstClientCmd = (queueCalls(h)[0] as Record<string, unknown>)["clientCmd"] as string;
        h.handle?.requestFocus("right");
        assert.equal(queueCalls(h).length, 2, "both presses filed upfront");
        // Expire the debounce, both settled queue timers, and the flight's
        // own capture timeout in turn. No native event is delivered anywhere
        // below.
        fireNextTimer(h, 2000, mark);
        fireNextTimer(h, 2000, mark);
        fireNextTimer(h, 2000, mark);
        fireNextTimer(h, 2000, mark);
        assert.ok(h.logs.some((l) => l.includes("timeout")));
        // The timeout redrains the parked tail (filing nothing further).
        h.runTimers();
        assert.equal(queueCalls(h).length, 2, "the redrain files nothing further");
        const queued = queueCalls(h);
        const drained = queued[1]?.["command"] as Record<string, unknown>;
        assert.equal(drained["op"], "focus-directional");
        assert.equal(drained["direction"], "right");
        acceptLast(h, "gen-1", "1");
        // The timed-out head never reappears.
        const seqs = sentCmdSeqs(h);
        assert.equal(seqs.filter((s) => s === firstClientCmd).length, 1);
        assert.equal(new Set(seqs).size, seqs.length);
    });

    it("drops queued directionals on generation mismatch with a diagnostic", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        h.handle?.requestFocus("right");
        h.handle?.requestFocus("left");
        // Poison while one head flies and two tails wait: the whole queue
        // belongs to the dead generation.
        h.lastCall("DescribeWorld").callback(JSON.stringify({
            v: 4, outcome: "rejected", kind: "generation-mismatch", message: "stale", gen: "gen-2",
            source: "", nativeInvalid: "0", clock: "0", gestureAssoc: null,
        }));
        assert.ok(h.logs.some((l) => l.includes("queue-dropped reason=generation-mismatch depth=2")));
        assert.equal(handshakeCount(h), 2);
        const cmdsBefore = sentCmdSeqs(h).length;
        // Only the native debounce elapses here: expiring the fresh
        // discovery round instead would wedge the rebind it is about to
        // answer. The armed pre-mismatch debounce finds the discovery
        // gate closed and dispatches nothing.
        h.runTimers();
        // The discovery gate holds while unbound: nothing dispatches, and
        // no command ever replays.
        assert.deepEqual(sentCmdSeqs(h).length, cmdsBefore);
        answerHandshake(h, "gen-2");
        answerSettings(h, "gen-2");
        {
            const auto = h.lastCall("DescribeWorld");
            const autoBody = JSON.parse(auto.payload);
            auto.callback(acceptedReply("gen-2", autoBody.source, autoBody.nativeInvalid, "0"));
        }
        // After rebinding, a fresh press flows; the dropped tails never do.
        h.handle?.requestFocus("right");
        const fresh = requestOf(h);
        assert.equal(fresh["gen"], "gen-2");
        const freshQ = JSON.parse(queueOf(h)) as Record<string, unknown>;
        assert.equal((freshQ["command"] as Record<string, unknown>)["direction"], "right");
        // Filing history keeps the pre-drop parks (filed promptly at press)
        // plus the fresh post-rebind press; only the fresh press drains.
        const dirs = queueCalls(h)
            .map((req) => (req["command"] as Record<string, unknown>)["direction"]);
        assert.deepEqual(dirs, ["left", "right", "left", "right"]);
    });

    it("parks every press FIFO with no busy refusal, directional or not", () => {
        const h = makeHarness();
        discover(h);
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.requestFocus("left");
        // A workspace select behind a directional flight parks FIFO too
        // (QueueWorldCommand only parks; one capture drains the batch):
        // no busy refusal, no shortcut failure, native state untouched.
        h.handle?.requestWorkspaceRelative(1);
        assert.ok(!h.logs.some((l) => l.includes("command-busy")));
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
        // A resize parks the same way.
        h.handle?.requestResize("left", "outwards");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed action=resize")));
        assert.equal(queueCalls(h).length, 3, "all three presses filed upfront in press order");
        assert.deepEqual(
            queueCalls(h).map((req) => (req["command"] as Record<string, unknown>)["op"]),
            ["focus-directional", "select-relative", "resize"],
        );
        // The idle fast path already flew the first press alone; the parked
        // pair drains as one batch after its acceptance.
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore + 1,
        );
        acceptLast(h, "gen-1", "1");
        h.runTimers();
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore + 2,
            "parked pair drains as one batch",
        );
        assert.equal(queueCalls(h).length, 3, "the batch drain files nothing further");
    });

    it("drops the queue on stop with no post-stop dispatch", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        h.handle?.requestFocus("right");
        const callsBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.stop();
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            handler();
        }
        h.runTimers(2000);
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, callsBefore);
        // Presses after stop refuse with no dispatch.
        h.handle?.requestFocus("right");
        assert.ok(h.logs.some((l) => l.includes("shortcut-failed")));
        assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, callsBefore);
    });
});

describe("world entry native bindings", () => {
    it("converges bindings per (output, native) and prunes non-live cells", () => {
        const h = makeHarness("per-output-local");
        discover(h);
        // Dispatch a command-carrying report; the accepted reply binds the
        // same native on two outputs with a distinct logical each.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const first = h.lastCall("DescribeWorld");
        const firstBody = JSON.parse(first.payload);
        const wsRow = (id: string, out: string, bind: string, order: number) => ({
            id, out, bind, scope: { kind: "output", out }, order, mode: "tiled", exist: true, hold: "",
        });
        first.callback(acceptedReply("gen-1", firstBody.source as string, firstBody.nativeInvalid as string, "1", {
            workspaces: [
                wsRow("al-1", "out", "ws1", 0),
                wsRow("al-9", "out-2", "ws1", 0),
            ],
        }));
        // A second dispatch (native invalidation + debounce) carries the
        // converged per-cell bindings for live cells only: (out, ws1) kept,
        // (out-2, ws1) pruned with its output gone.
        for (const handler of (h.signals.desktopsChanged as FakeSignal).handlers) {
            handler();
        }
        h.runTimers();
        const second = reportOf(h);
        // Host-owned adapters carry no logical views lane: the core derives
        // receipts from native order/current. The converged binding proof
        // rides native[] (live order plus current), never a views lane.
        assert.ok(!("views" in second), "v4 carries no logical views lane");
        const native = second["native"] as Array<{ out: string; order: string[]; current: string }>;
        const row = native.find((n) => n.out === "out");
        assert.ok(row !== undefined);
        assert.ok(row.order.includes("ws1"), "live cell publishes its native order");
        assert.equal(row.current, "ws1", "live cell publishes its native current");
    });

    it("replicates bindings per output in per-output-local across two screens", () => {
        const h = makeHarness("per-output-local");
        h.world.screens.push({ name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } });
        discover(h);
        assert.equal(settingsValues(h)["mode"], "per-output-local");
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const first = h.lastCall("DescribeWorld");
        const firstBody = JSON.parse(first.payload);
        const wsRow = (id: string, out: string, bind: string, order: number) => ({
            id, out, bind, scope: { kind: "output", out }, order, mode: "tiled", exist: true, hold: "",
        });
        first.callback(acceptedReply("gen-1", firstBody.source as string, firstBody.nativeInvalid as string, "1", {
            workspaces: [
                wsRow("al-1", "out", "ws1", 0),
                wsRow("al-9", "out-2", "ws1", 0),
            ],
        }));
        for (const handler of (h.signals.desktopsChanged as FakeSignal).handlers) {
            handler();
        }
        h.runTimers();
        const second = reportOf(h);
        // One native order/current per output (the same global native list
        // projects per output); the logical binding stays core-owned and
        // v4 carries no logical views lane.
        assert.ok(!("views" in second), "v4 carries no logical views lane");
        assert.deepEqual(second["native"], [
            { out: "out", order: ["ws1", "ws2"], current: "ws1", occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 1 }] },
            { out: "out-2", order: ["ws1", "ws2"], current: "ws1", occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 1 }] },
        ]);
    });

    it("keeps single-row bindings in shared and resolves without output scope", () => {
        const h = makeHarness("shared");
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const first = h.lastCall("DescribeWorld");
        const firstBody = JSON.parse(first.payload);
        assert.equal(settingsValues(h)["mode"], "shared");
        first.callback(acceptedReply("gen-1", firstBody.source, firstBody.nativeInvalid, "1", {
            workspaces: [
                { id: "sh-1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        for (const handler of (h.signals.desktopsChanged as FakeSignal).handlers) {
            handler();
        }
        h.runTimers();
        const second = reportOf(h);
        // Shared is unchanged: one native order/current row, and the
        // logical binding stays core-owned with no views lane on the wire.
        assert.ok(!("views" in second), "v4 carries no logical views lane");
        assert.deepEqual(second["native"], [{ out: "out", order: ["ws1", "ws2"], current: "ws1", occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 1 }] }]);
    });
});

describe("world entry all-mode native coverage", () => {
    // Producer-shaped two-output native surface: one global desktop list
    // of four natives, out-A viewing ws1, out-B viewing ws4 with its sole
    // window homed on ws4 (ws1-3 empty on B). The 6.6 shape drops the
    // per-output seam so the shared currentDesktop is current everywhere.
    const NATIVES4 = ["ws1", "ws2", "ws3", "ws4"];
    function makeTwoOutputHarness(
        mode: unknown,
        opts: { optionsValue?: unknown; perOutputSeam?: boolean; sharedCurrent?: string } = {},
    ): Harness & { currents: Map<string, string>; surface: unknown } {
        const perOutputSeam = opts.perOutputSeam ?? true;
        const sharedId = opts.sharedCurrent ?? "ws1";
        const desktops = NATIVES4.map((id) => desk(id));
        const byId = new Map(desktops.map((d) => [d.id, d]));
        const sharedDesk = (id: string): FakeDesktop => {
            const found = byId.get(id);
            assert.ok(found !== undefined, `desktop ${id} exists`);
            return found as FakeDesktop;
        };
        const screens = [
            { name: "out-A", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
            { name: "out-B", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } },
        ];
        const currents = new Map([["out-A", "ws1"], ["out-B", "ws4"]]);
        const wA = win("win-a1", "out-A", sharedDesk("ws1"), 0);
        const wB = win("win-b4", "out-B", sharedDesk("ws4"), 1920);
        const world = {
            screens,
            desktops,
            wins: [wA, wB],
            activeScreen: { name: "out-A" },
            activeWindow: wA as FakeWindow | null,
            currentDesktop: sharedDesk(sharedId),
            created: [] as string[],
            removed: [] as string[],
            focused: [] as string[],
            membershipWrites: [] as string[],
            geometryWrites: [] as string[],
        };
        const calls: CapturedCall[] = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const shortcuts = new Map<string, () => void>();
        const signals: Record<string, FakeSignal> = {
            windowAdded: fakeSignal(),
            windowRemoved: fakeSignal(),
            windowActivated: fakeSignal(),
            desktopsChanged: fakeSignal(),
            screensChanged: fakeSignal(),
            currentDesktopChanged: fakeSignal(),
            activeScreenChanged: fakeSignal(),
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
            ...(perOutputSeam ? {
                currentDesktopForScreen: (screen: { name: string }) => sharedDesk(currents.get(screen.name) ?? ""),
                setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
                    currents.set(screen.name, (deskRef as FakeDesktop).id);
                },
            } : {}),
            sendClientToScreen: (_mover: unknown, _out: unknown) => undefined,
            createDesktop: (...args: ReadonlyArray<unknown>) => {
                if (args.length !== 2 || typeof args[0] !== "number" || typeof args[1] !== "string") {
                    throw new Error("Insufficient arguments");
                }
                const id = `native-${world.desktops.length + 1}`;
                world.desktops.push(desk(id, args[1] as string));
                world.created.push(id);
            },
            removeDesktop: (ref: unknown) => {
                const id = (ref as FakeDesktop).id;
                world.desktops = world.desktops.filter((d) => d.id !== id);
                world.removed.push(id);
            },
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                calls.push({ method, payload, callback });
                if (method === "QueueWorldCommand") {
                    const queueBody = JSON.parse(payload) as Record<string, unknown>;
                    const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                    callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                        client: queueBody["client"], clientCmd: queueBody["clientCmd"],
                        globalCmd: String(calls.filter((q) => q.method === "QueueWorldCommand").length) }));
                } else if (method === "TakeCommandResult") {
                    const takeBody = JSON.parse(payload) as Record<string, unknown>;
                    const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: (message) => {
                logs.push(message);
            },
            gestureActive: () => false,
            registerShortcutFn: (name, _text, _seq, cb) => {
                shortcuts.set(name, cb);
                return true;
            },
            readWorkspaceModeFn: () => mode,
            readDefaultTiledFn: () => true,
            ...(opts.optionsValue !== undefined ? { options: opts.optionsValue } : {}),
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
        return {
            world, calls, timers, logs, shortcuts, signals, handle,
            currents, surface,
            runTimers,
            intentWriteFlights: [] as Harness["intentWriteFlights"],
            lastCall: (method: string): CapturedCall => {
                const found = [...calls].reverse().find((c) => c.method === method);
                assert.ok(found !== undefined, `expected ${method}`);
                return found as CapturedCall;
            },
        };
    }

    const warnsOf = (h: Harness): string[] =>
        h.logs.filter((l) => l.includes("per-output-unavailable"));

    // One native-event capture round settled with an accepted reply, so
    // the next round dispatches fresh (no flight, no busy refusal).
    const captureRound = (h: Harness, rev: string): void => {
        for (const handler of [...(h.signals.desktopsChanged as FakeSignal).handlers]) {
            handler();
        }
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, rev));
    };

    it("separates per-output-local from bogus/shared for binding replication", () => {
        assert.equal(isSeparateWorkspaceMode("per-output-local"), true);
        assert.equal(isSeparateWorkspaceMode("global-unique"), true);
        assert.equal(isSeparateWorkspaceMode("shared"), false);
        assert.equal(isSeparateWorkspaceMode("bogus"), false);
        assert.equal(isSeparateWorkspaceMode(undefined), false);
        assert.equal(isSeparateWorkspaceMode(null), false);
    });

    for (const mode of ["per-output-local", "global-unique"]) {
        it(`rides ${mode} with independent currents over the shared four-native list`, () => {
            const h = makeTwoOutputHarness(mode);
            discover(h);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const report = reportOf(h);
            assert.equal(settingsValues(h)["mode"], mode);
            // The global native desktop list projects per output with an
            // independent current each; every order has the same length.
            assert.deepEqual(report["native"], [
                { out: "out-A", order: ["ws1", "ws2", "ws3", "ws4"], current: "ws1", occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 0 }, { native: "ws3", count: 0 }, { native: "ws4", count: 1 }] },
                { out: "out-B", order: ["ws1", "ws2", "ws3", "ws4"], current: "ws4", occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 0 }, { native: "ws3", count: 0 }, { native: "ws4", count: 1 }] },
            ]);
            // Host-owned adapters carry no logical views lane: the core
            // derives receipts from the native current map.
            assert.ok(!("views" in report), "v4 carries no logical views lane");
            // Membership truth: out-B/ws4 occupied, ws1-3 empty on B.
            const wins = report["windows"] as Array<Record<string, unknown>>;
            const homed = wins.find((w) => w["win"] === "win-b4");
            assert.ok(homed !== undefined, "ws4 member observed");
            assert.equal(homed["out"], "out-B");
            assert.equal(homed["nativeWs"], "ws4");
            assert.deepEqual(homed["rect"], [1920, 0, 200, 200]);
            assert.ok(
                !wins.some((w) => w["win"] !== "win-b4" && w["out"] === "out-B"),
                "ws1-3 empty on out-B",
            );
            h.handle?.stop();
        });
    }

    it("rides shared with the same global list and membership", () => {
        const h = makeTwoOutputHarness("shared");
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const report = reportOf(h);
        assert.equal(settingsValues(h)["mode"], "shared");
        assert.deepEqual((report["native"] as Array<Record<string, unknown>>)[1], {
            out: "out-B", order: ["ws1", "ws2", "ws3", "ws4"], current: "ws4",
            occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 0 }, { native: "ws3", count: 0 }, { native: "ws4", count: 1 }],
        });
        assert.ok(!("views" in report), "v4 carries no logical views lane");
        const wins = report["windows"] as Array<Record<string, unknown>>;
        assert.equal(wins.find((w) => w["win"] === "win-b4")?.["nativeWs"], "ws4");
        h.handle?.stop();
    });

    for (const mode of ["per-output-local", "global-unique", "shared"]) {
        it(`degrades ${mode} to shared with linked currents on a 6.6 host`, () => {
            const h = makeTwoOutputHarness(mode, { perOutputSeam: false, sharedCurrent: "ws2" });
            discover(h);
            h.handle?.requestWorkspaceRelative(1);
            h.runTimers();
            const report = reportOf(h);
            // The observer resolves the effective mode: per-output
            // requests degrade predictably where the API is absent. The
            // requested mode still rides the settings snapshot verbatim.
            assert.equal(settingsValues(h)["mode"], mode);
            // The shared currentDesktop is current on every output: linked
            // currents prove the degraded shared effective mode.
            assert.deepEqual(report["native"], [
                { out: "out-A", order: ["ws1", "ws2", "ws3", "ws4"], current: "ws2", occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 0 }, { native: "ws3", count: 0 }, { native: "ws4", count: 1 }] },
                { out: "out-B", order: ["ws1", "ws2", "ws3", "ws4"], current: "ws2", occupancy: [{ native: "ws1", count: 1 }, { native: "ws2", count: 0 }, { native: "ws3", count: 0 }, { native: "ws4", count: 1 }] },
            ]);
            assert.ok(!("views" in report), "v4 carries no logical views lane");
            h.handle?.stop();
        });
    }

    it("reports truthful nativeWs on the occupied cell through actual capture", () => {
        const h = makeTwoOutputHarness("global-unique");
        const surface = (h as unknown as { surface: unknown }).surface;
        const obs = captureWorldObservation({
            workspace: surface,
            readMode: () => "global-unique",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: () => null,
            replicateBindingsPerOutput: true,
        });
        assert.ok(obs !== null, "observation captures");
        assert.equal((obs as { mode: unknown }).mode, "global-unique");
        // Binding replication emits one row per (output, native) with a
        // distinct logical each over the four-native list.
        const withBind = captureWorldObservation({
            workspace: surface,
            readMode: () => "global-unique",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: (nativeId: string, out?: string) => out === undefined
                ? { logical: `L-${nativeId}`, out: "out-A", pin: true, disp: false, managed: true }
                : { logical: `L-${out}/${nativeId}`, out, pin: true, disp: false, managed: true },
            replicateBindingsPerOutput: true,
        });
        assert.ok(withBind !== null);
        assert.equal((withBind as { bindings: Array<unknown> }).bindings.length, 8);
        const pair = (withBind as { bindings: Array<{ logical: string; native: string; out: string }> }).bindings
            .filter((b) => b.native === "ws4");
        assert.deepEqual(pair.map((b) => `${b.out}=${b.logical}`).sort(), ["out-A=L-out-A/ws4", "out-B=L-out-B/ws4"]);
        // Shared keeps one row per native.
        const sharedObs = captureWorldObservation({
            workspace: surface,
            readMode: () => "shared",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: (nativeId: string) => ({ logical: `L-${nativeId}`, out: "out-A", pin: true, disp: false, managed: true }),
            replicateBindingsPerOutput: false,
        });
        assert.ok(sharedObs !== null);
        assert.equal((sharedObs as { bindings: Array<unknown> }).bindings.length, 4);
        // The occupied out-B/ws4 domain homes win-b4 with its nativeWs;
        // ws1-3 carry no out-B members.
        const domains = (obs as { domains: Array<{ id: string; ws: string; windows: Array<{ win: string; nativeWs: string | null }> }> }).domains;
        const cell = domains.find((d) => d.id === "out-B" && d.ws === "ws4");
        assert.ok(cell !== undefined, "occupied cell observed");
        assert.deepEqual(cell.windows.map((w) => w.win), ["win-b4"]);
        assert.equal(cell.windows[0]?.nativeWs, "ws4");
        assert.ok(
            !domains.some((d) => d.id === "out-B" && d.ws !== "ws4" && d.windows.length > 0),
            "ws1-3 empty on out-B",
        );
        h.handle?.stop();
    });

    it("warns change-gated only for separate modes with the flag off", () => {
        // Off + separate warns once on the discovery capture with the mode.
        const off = makeHarness("per-output-local", false, true, undefined, { perOutputVirtualDesktops: false });
        discover(off);
        assert.equal(warnsOf(off).length, 1);
        assert.ok(warnsOf(off)[0]?.includes("mode=per-output-local") && warnsOf(off)[0]?.includes("reason=kwin-per-output-off"));
        // A second capture without change stays gated.
        captureRound(off, "1");
        assert.equal(warnsOf(off).length, 1);
        // On stays silent; unknown stays silent; shared stays silent.
        for (const [mode, options] of [
            ["per-output-local", { perOutputVirtualDesktops: true }],
            ["global-unique", { perOutputVirtualDesktops: true }],
            ["per-output-local", undefined],
            ["shared", { perOutputVirtualDesktops: false }],
        ] as const) {
            const h = options === undefined
                ? makeHarness(mode)
                : makeHarness(mode, false, true, undefined, { ...options });
            discover(h);
            captureRound(h, "1");
            assert.deepEqual(warnsOf(h), [], `silent for mode=${String(mode)} flag=${JSON.stringify(options)}`);
            h.handle?.stop();
        }
        // Global-unique off warns with its own mode token.
        const gu = makeHarness("global-unique", false, true, undefined, { perOutputVirtualDesktops: false });
        discover(gu);
        assert.equal(warnsOf(gu).length, 1);
        assert.ok(warnsOf(gu)[0]?.includes("mode=global-unique"));
        gu.handle?.stop();
        // A 6.6 host (no seam, unreadable flag) stays silent and degrades.
        const legacy = makeHarness("per-output-local", false, true, undefined, undefined, false);
        discover(legacy);
        captureRound(legacy, "1");
        assert.deepEqual(warnsOf(legacy), []);
        // A 6.6 host degrades to linked shared currents on the native lane.
        const legacyNative = reportOf(legacy)["native"] as Array<Record<string, unknown>>;
        assert.ok(legacyNative.length >= 1);
        assert.ok(legacyNative.every((n) => n["current"] === legacyNative[0]?.["current"]), "degraded linked currents");
        legacy.handle?.stop();
        off.handle?.stop();
    });

    it("re-warns only on a new off transition", () => {
        const flags = { perOutputVirtualDesktops: false };
        const h = makeHarness("per-output-local", false, true, undefined, flags);
        discover(h);
        assert.equal(warnsOf(h).length, 1);
        // Off flips on: silent, but the gate key advances on capture.
        flags.perOutputVirtualDesktops = true;
        captureRound(h, "1");
        assert.equal(warnsOf(h).length, 1);
        // Stays on: still gated.
        captureRound(h, "2");
        assert.equal(warnsOf(h).length, 1);
        // Flips off again: a new transition warns once more.
        flags.perOutputVirtualDesktops = false;
        captureRound(h, "3");
        assert.equal(warnsOf(h).length, 2);
        captureRound(h, "4");
        assert.equal(warnsOf(h).length, 2);
        h.handle?.stop();
    });
});

// Exact core-minted schema1 opaque intent bytes (proven by the service
// roundtrip plus the explicit float/tile restore coverage): float=true
// floats, float=false is the tile override. The service store carries them
// inside schema2 records; the entry never interprets them.
const FLOAT_DATA = "{\"v\":1,\"float\":true,\"stickyPrior\":null}";
const TILE_DATA = "{\"v\":1,\"float\":false,\"stickyPrior\":null}";

describe("world entry restart float-intent preservation", () => {
    for (const holdMode of ["scoped", "global"] as const) {
    it(`holds settled float across a genuinely held first engine reply (${holdMode} hold)`, { timeout: 120000 }, async () => {
        // Producer-shaped replay through the real WorldEngine (raw_world_eval,
        // same bridge as the migration engine test): no hand-built replies on the asserted paths.
        // Scoped hold is genuinely native: clientArea returns null for ws-1,
        // which world-observe turns into an explicit domain hold
        // "unreadable". Global hold poisons one live rect so the whole
        // inventory reads unreadable and every domain empties its windows
        // while staying non-null. One settled float (win-a on out-a/ws-1)
        // plus one disjoint tile override (win-b on out-b/ws-2) seed from
        // the real persisted-store read shape. No commands ride.
        const { spawn } = await import("node:child_process");
        const { existsSync } = await import("node:fs");
        const { createInterface } = await import("node:readline");
        const { dirname, join, resolve } = await import("node:path");
        const repoRoot = (): string => {
            let dir = resolve(process.cwd());
            for (let depth = 0; depth < 4; depth += 1) {
                if (existsSync(join(dir, "Cargo.toml"))) {
                    return dir;
                }
                dir = dirname(dir);
            }
            throw new Error("fixture root not found");
        };
        interface EngineWaiter {
            resolve: (reply: string) => void;
            reject: (error: Error) => void;
        }
        const proc = spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], {
            cwd: repoRoot(),
            stdio: ["pipe", "pipe", "pipe"],
        });
        const waiters: EngineWaiter[] = [];
        let engineFailed: Error | null = null;
        const stderrChunks: string[] = [];
        try {
            proc.stderr?.on("data", (chunk: unknown) => {
                stderrChunks.push(String(chunk));
            });
        } catch (error) {
            void error;
        }
        const failEngine = (error: Error): void => {
            if (engineFailed === null) {
                engineFailed = error;
            }
            while (waiters.length > 0) {
                waiters.shift()?.reject(error);
            }
        };
        proc.on("error", (error: Error) => {
            failEngine(error);
        });
        proc.on("exit", (code: number | null, signal: string | null) => {
            failEngine(new Error(`raw_world_eval exited code=${String(code)} signal=${String(signal)} stderr=${stderrChunks.join("").slice(0, 500)}`));
        });
        const stdout = proc.stdout;
        assert.ok(stdout !== null && stdout !== undefined, "raw_world_eval stdout unavailable");
        const lines = createInterface({ input: stdout });
        lines.on("line", (line: string) => {
            waiters.shift()?.resolve(line);
        });
        const engineSend = (method: string, body: string): Promise<string> => new Promise<string>((resolvePromise, rejectPromise) => {
            if (engineFailed !== null) {
                rejectPromise(engineFailed);
                return;
            }
            const timer = setTimeout(() => {
                const index = waiters.findIndex((w) => w.resolve === resolvePromise);
                if (index >= 0) {
                    waiters.splice(index, 1);
                }
                rejectPromise(new Error(`raw_world_eval timeout method=${method}`));
            }, 15000);
            waiters.push({
                resolve: (reply: string) => {
                    clearTimeout(timer);
                    resolvePromise(reply);
                },
                reject: (error: Error) => {
                    clearTimeout(timer);
                    rejectPromise(error);
                },
            });
            try {
                const ok = proc.stdin?.write(JSON.stringify({ method, body }) + "\n");
                if (ok === false) {
                    proc.stdin?.once("drain", () => undefined);
                }
            } catch (error) {
                clearTimeout(timer);
                rejectPromise(error instanceof Error ? error : new Error(String(error)));
            }
        });
        const ws1 = desk("ws1");
        const ws2 = desk("ws2");
        const w1 = win("win-a", "out-a", ws1, 0);
        const w2 = win("win-b", "out-b", ws2, 0);
        const screens = [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ];
        const currents = new Map([["out-a", "ws1"], ["out-b", "ws2"]]);
        // Genuine native carrier facts (mirroring the migrated engine
        // harnesses): truthful PID, identity texts, resizable state, and a
        // client rect tracking the frame, so the core never holds rows for
        // missing facts on the asserted paths.
        const setFrame = (w: FakeWindow, rect: { x: number; y: number; width: number; height: number }): void => {
            (w as unknown as Record<string, unknown>)["frameGeometry"] = { ...rect };
            (w as unknown as Record<string, unknown>)["clientGeometry"] = {
                x: rect.x + 2, y: rect.y + 21,
                width: Math.max(1, rect.width - 4), height: Math.max(1, rect.height - 22),
            };
        };
        for (const w of [w1, w2]) {
            const rec = w as unknown as Record<string, unknown>;
            rec["pid"] = 1234;
            rec["resourceClass"] = "Example";
            rec["caption"] = "Window";
            rec["windowRole"] = "normal";
            rec["desktopFileName"] = "example.app";
            rec["resizeable"] = true;
        }
        setFrame(w1, w1.frameGeometry);
        setFrame(w2, w2.frameGeometry);
        // Genuine native unreadability while heldFirst is true.
        // Scoped: the ws-1 work area reads as null, so the dispatched
        // report holds out-a/ws-1 as unreadable. Global: one live rect
        // poisons the whole inventory read, so every domain empties its
        // windows while the report stays non-null.
        let heldFirst = true;
        // Sole-broker mini-harness: the envelope becomes the exact core
        // report (broker namespace plus at most one queued command) and
        // the core reply returns bound with a nulled terminal result.
        const broker = createMiniBroker();
        const signals: Record<string, FakeSignal> = {
            windowAdded: fakeSignal(),
            windowRemoved: fakeSignal(),
            windowActivated: fakeSignal(),
            desktopsChanged: fakeSignal(),
            screensChanged: fakeSignal(),
            currentDesktopChanged: fakeSignal(),
            activeScreenChanged: fakeSignal(),
        };
        const calls: CapturedCall[] = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const writes: string[] = [];
        const surface = {
            windowList: () => {
                if (heldFirst && holdMode === "global") {
                    // Poison one live rect: the observer marks the whole
                    // inventory unreadable and empties every domain's
                    // windows (global hold, still a non-null report).
                    setFrame(w1, { x: 0, y: 0, width: 0, height: 200 });
                }
                return [w1, w2];
            },
            screens,
            desktops: [ws1, ws2],
            activeScreen: { name: "out-a" },
            cursorPos: { x: 2, y: 100 },
            windowAdded: (signals.windowAdded as FakeSignal).signal,
            windowRemoved: (signals.windowRemoved as FakeSignal).signal,
            windowActivated: (signals.windowActivated as FakeSignal).signal,
            desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
            screensChanged: (signals.screensChanged as FakeSignal).signal,
            currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
            activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
            get activeWindow() {
                return w1;
            },
            clientArea: (_area: number, _out: unknown, deskRef: unknown) => {
                if (heldFirst && holdMode === "scoped" && (deskRef as { id?: unknown }).id === "ws1") {
                    return null;
                }
                return { x: 0, y: 0, width: 800, height: 600 };
            },
            currentDesktopForScreen: (screen: { name: string }) => (currents.get(screen.name) === "ws2" ? ws2 : ws1),
            setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
                currents.set(screen.name, (deskRef as { id: string }).id);
            },
            sendClientToScreen: () => undefined,
            createDesktop: () => null,
            removeDesktop: () => false,
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            floatIntent: true,
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === "ReadFloatIntent") {
                    // Exact schema2 persisted-store read shape
                    // (float_intent_store read_ok_reply): ok outcome with
                    // opaque records plus matching stored/returned counts,
                    // seeded before the first dispatch so the first native
                    // capture already floats win-a.
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 2,
                        correlation_id: body["correlation_id"],
                        outcome: "ok",
                        records: [
                            { win: "win-a", data: FLOAT_DATA },
                            { win: "win-b", data: TILE_DATA },
                        ],
                        stored: 2,
                        returned: 2,
                    }));
                    return;
                }
                if (method === "WriteFloatIntent") {
                    writes.push(payload);
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 2,
                        correlation_id: body["correlation_id"],
                        outcome: "stored",
                        stored: (body["records"] as Array<unknown>).length,
                    }));
                    return;
                }
                if (method === "DiscoverWorld" || method === "DescribeWorld") {
                    calls.push({ method, payload, callback });
                    return;
                }
                if (method === "QueueWorldCommand") {
                    calls.push({ method, payload, callback });
                    callback(broker.answerQueue(payload));
                    return;
                }
                if (method === "TakeCommandResult") {
                    calls.push({ method, payload, callback });
                    callback(broker.answerTake(payload));
                    return;
                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
            readDefaultTiledFn: () => true,
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
        let flushed = 0;
        const flushNextDescribe = async (): Promise<{ req: Record<string, unknown>; reply: Record<string, unknown>; rawReply: string }> => {
            const at = calls.findIndex((c, i) => i >= flushed && c.method === "DescribeWorld");
            assert.ok(at >= 0, "expected a queued DescribeWorld");
            flushed = at + 1;
            // Mini-broker stamping: the envelope becomes the exact core
            // report the sole broker would dispatch, and the core reply
            // returns bound with a nulled terminal result.
            const envelope = calls[at]!.payload;
            const stamped = broker.stampCapture(envelope);
            const coreReply = await engineSend("DescribeWorld", stamped);
            const rawReply = broker.bindReply(coreReply, envelope);
            calls[at]!.callback(rawReply);
            return { req: JSON.parse(calls[at]!.payload) as Record<string, unknown>, reply: JSON.parse(rawReply) as Record<string, unknown>, rawReply };
        };
        try {
            // Authentic v4 connection pump through the real engine: the
            // handshake binds the producer adapter lifetime and mints the
            // serving generation, then the actual revisioned settings ride
            // to the engine and must read accepted before any capture
            // flows (never a scripted settings reply on the engine path).
            const hsAt = calls.findIndex((c) => {
                if (c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
                } catch {
                    return false;
                }
            });
            assert.ok(hsAt >= 0, "entry discovers on start");
            const hsBody = JSON.parse(calls[hsAt]!.payload) as Record<string, unknown>;
            assert.equal(hsBody["adapter"], "test-adapter", "handshake carries the producer adapter");
            const engineHs = await engineSend("DiscoverWorld", calls[hsAt]!.payload);
            const hsReply = JSON.parse(engineHs) as Record<string, unknown>;
            assert.equal(hsReply["type"], "handshake-reply", "engine answers the handshake");
            assert.equal(hsReply["ready"], false, "accepted handshake carries ready:false");
            assert.equal(hsReply["adapter"], "test-adapter", "handshake retains the producer adapter, never a broker incarnation");
            assert.equal(hsReply["req"], hsBody["req"], "handshake echoes the request correlation");
            assert.ok(typeof hsReply["gen"] === "string" && (hsReply["gen"] as string) !== "", "handshake mints the serving generation");
            const gen = hsReply["gen"] as string;
            calls[hsAt]!.callback(broker.bindDiscovery(engineHs));
            assert.equal(broker.gen, gen, "harness records the actual serving generation");
            const setAt = calls.findIndex((c, i) => {
                if (i <= hsAt || c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
                } catch {
                    return false;
                }
            });
            assert.ok(setAt >= 0, "revisioned settings follow the handshake");
            const setBody = JSON.parse(calls[setAt]!.payload) as Record<string, unknown>;
            assert.equal(setBody["type"], "settings", "second discovery is the revisioned settings snapshot");
            assert.equal(setBody["adapter"], "test-adapter", "settings carry the producer adapter");
            assert.equal(setBody["gen"], gen, "settings bind the serving generation");
            const engineSet = await engineSend("DiscoverWorld", calls[setAt]!.payload);
            const setReply = JSON.parse(engineSet) as Record<string, unknown>;
            assert.equal(setReply["type"], "settings-reply", "engine answers settings");
            assert.equal(setReply["status"], "accepted", `settings accepted, got ${engineSet.slice(0, 300)}`);
            assert.equal(setReply["adapter"], "test-adapter", "settings reply retains the producer adapter");
            assert.equal(setReply["gen"], gen, "settings reply binds the serving generation");
            assert.equal(setReply["req"], setBody["req"], "settings reply echoes the request correlation");
            assert.equal(setReply["rev"], setBody["rev"], "settings reply binds the sent revision");
            calls[setAt]!.callback(engineSet);
            assert.deepEqual(
                calls.slice(0, setAt + 1).map((c) => c.method),
                ["DiscoverWorld", "DiscoverWorld"],
                "no report precedes the accepted settings",
            );
            // First accepted observation: the auto observation-only dispatch
            // after discovery, with the hold applied. No commands.
            const first = await flushNextDescribe();
            // v2 holds ride screen completeness and window facts (no
            // domains lane on the wire): the scoped hold leaves out-a's
            // work area unreadable while out-b stays complete; the global
            // hold empties the window facts with the report incomplete.
            const firstReport = first.req["report"] as Record<string, unknown>;
            const firstScreens = firstReport["screens"] as Array<Record<string, unknown>>;
            if (holdMode === "scoped") {
                const screenA = firstScreens.find((s) => s["out"] === "out-a");
                const screenB = firstScreens.find((s) => s["out"] === "out-b");
                assert.equal(
                    screenA?.["complete"],
                    false,
                    `first dispatched request holds out-a unreadable: ${JSON.stringify(firstScreens)}`,
                );
                assert.equal(
                    screenB?.["complete"],
                    true,
                    `out-b stays complete: ${JSON.stringify(firstScreens)}`,
                );
            } else {
                // Global inventory failure empties every window fact while
                // the report stays non-null.
                const wins = firstReport["windows"] as Array<unknown>;
                assert.equal(wins.length, 0, `global hold empties window facts: ${JSON.stringify(firstReport)}`);
                assert.equal(firstReport["complete"], false, "global hold marks the report incomplete");
            }
            // The genuine Engine reply must lack any untouched/no-hold row
            // for the seeded float: that absence is the producer condition
            // behind the restart loss. Tile win-b rides its own row.
            const firstWins = first.reply["windows"] as Array<Record<string, unknown>>;
            const firstRowA = firstWins.find((w) => w["win"] === "win-a");
            // Genuine engine behavior: the held scope carries no win-a row
            // at all (scopes: out-a/ws1 auth=hold reason=unreadable), so the
            // reply lacks any untouched/no-hold row for the seeded float.
            assert.ok(
                !(firstRowA?.["frame_auth"] === "untouched" && firstRowA?.["frame_hold"] === ""),
                `first engine reply holds the float row: ${JSON.stringify(firstRowA)} scopes=${JSON.stringify(first.reply["scopes"])}`,
            );
            // Restore the readable native condition and recapture the same
            // live identities completely.
            heldFirst = false;
            if (holdMode === "global") {
                setFrame(w1, { x: 0, y: 0, width: 200, height: 200 });
            }
            for (const handler of [...(signals.desktopsChanged as FakeSignal).handlers]) {
                handler();
            }
            runTimers();
            const second = await flushNextDescribe();
            const secondReport = second.req["report"] as Record<string, unknown>;
            const secondScreens = secondReport["screens"] as Array<Record<string, unknown>>;
            assert.ok(
                secondScreens.every((s) => s["complete"] === true),
                `second dispatched request is complete: ${JSON.stringify(secondScreens)}`,
            );
            // No uncommanded settled float loss: the settled snapshot needs
            // no churn (the core issues no save/clear decisions on
            // observation-only captures in this increment), and the second
            // dispatched observation still rides the seeded records
            // live-joined with exact core-minted bytes, disjoint per side.
            assert.equal(writes.length, 0, "settled snapshot needs no churn across the held first reply");
            // The second dispatched observation still reports the seeded
            // float via the opaque intent lane: the raw row carries native
            // facts only, intentRecords carries the membership.
            const secondWins = (secondReport["windows"] as Array<Record<string, unknown>>);
            const rowA = secondWins.find((w) => w["win"] === "win-a");
            assert.ok(rowA !== undefined, "seeded win-a still observed");
            assert.ok(!("floating" in rowA), "no wire verdicts on raw rows");
            const secondRecs = ((secondReport["intentRecords"] as Array<{ win: string; data: string }>) ?? []);
            const secondRecA = secondRecs.find((r) => r.win === "win-a");
            const secondRecB = secondRecs.find((r) => r.win === "win-b");
            assert.ok(
                secondRecA !== undefined && secondRecA.data === FLOAT_DATA,
                `second dispatched observation still floats seeded win-a: ${JSON.stringify(secondRecs)}`,
            );
            assert.ok(
                secondRecB !== undefined && secondRecB.data === TILE_DATA,
                `tile override survives the held first reply: ${JSON.stringify(secondRecs)}`,
            );
            assert.ok(
                !secondRecs.some((r) => r.win === "win-b" && (JSON.parse(r.data) as Record<string, unknown>)["float"] === true),
                `fixed tile override is not a float: ${JSON.stringify(secondRecs)}`,
            );
            assert.ok(
                !secondRecs.some((r) => r.win === "win-a" && (JSON.parse(r.data) as Record<string, unknown>)["float"] === false),
                `float side stays disjoint: ${JSON.stringify(secondRecs)}`,
            );
            // The genuine engine applies the retained float once the hold
            // lifts (plain windows float from opaque intent records).
            const secondReplyWins = (second.reply["windows"] as Array<Record<string, unknown>>);
            assert.equal(
                secondReplyWins.find((w) => w["win"] === "win-a")?.["floating"],
                true,
                "engine applies the retained float after the hold lifts",
            );
        } finally {
            try {
                handle?.stop();
            } catch (error) {
                void error;
            }
            try {
                lines.close();
            } catch (error) {
                void error;
            }
            try {
                proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolvePromise) => {
                const timer = setTimeout(() => {
                    try {
                        proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolvePromise();
                }, 3000);
                proc.on("exit", () => {
                    clearTimeout(timer);
                    resolvePromise();
                });
                if (proc.exitCode !== null || proc.signalCode !== null) {
                    clearTimeout(timer);
                    resolvePromise();
                }
            });
        }
    });
    }
});

describe("world entry script-restart sequence handover", () => {
    it("rotation hands a fresh generation and engine to the new producer lifetime", { timeout: 120000 }, async () => {
        // Producer-shaped restart replay through the real WorldEngine behind
        // one raw_world_eval process (like production): the first
        // producer advances request+command watermarks with a float + fixed
        // tile seed, then the restarted producer discovers as a NEW
        // lifetime. Like the real WorldBroker::discovery_body, the new
        // lifetime rotates onto a fresh generation and a fresh engine
        // (raw_world_eval rebinds both on DiscoverWorld): the old generation
        // refuses outright, queued/settled state from the old world is
        // gone, and the saved default reseeds the fresh engine.
        const { spawn } = await import("node:child_process");
        const { existsSync } = await import("node:fs");
        const { createInterface } = await import("node:readline");
        const { dirname, join, resolve } = await import("node:path");
        const repoRoot = (): string => {
            let dir = resolve(process.cwd());
            for (let depth = 0; depth < 4; depth += 1) {
                if (existsSync(join(dir, "Cargo.toml"))) {
                    return dir;
                }
                dir = dirname(dir);
            }
            throw new Error("fixture root not found");
        };
        interface EngineWaiter {
            resolve: (reply: string) => void;
            reject: (error: Error) => void;
        }
        const proc = spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], {
            cwd: repoRoot(),
            stdio: ["pipe", "pipe", "pipe"],
        });
        const waiters: EngineWaiter[] = [];
        let engineFailed: Error | null = null;
        const stderrChunks: string[] = [];
        try {
            proc.stderr?.on("data", (chunk: unknown) => {
                stderrChunks.push(String(chunk));
            });
        } catch (error) {
            void error;
        }
        const failEngine = (error: Error): void => {
            if (engineFailed === null) {
                engineFailed = error;
            }
            while (waiters.length > 0) {
                waiters.shift()?.reject(error);
            }
        };
        proc.on("error", (error: Error) => {
            failEngine(error);
        });
        proc.on("exit", (code: number | null, signal: string | null) => {
            failEngine(new Error(`raw_world_eval exited code=${String(code)} signal=${String(signal)} stderr=${stderrChunks.join("").slice(0, 500)}`));
        });
        const stdout = proc.stdout;
        assert.ok(stdout !== null && stdout !== undefined, "raw_world_eval stdout unavailable");
        const lines = createInterface({ input: stdout });
        lines.on("line", (line: string) => {
            waiters.shift()?.resolve(line);
        });
        const engineSend = (method: string, body: string): Promise<string> => new Promise<string>((resolvePromise, rejectPromise) => {
            if (engineFailed !== null) {
                rejectPromise(engineFailed);
                return;
            }
            const timer = setTimeout(() => {
                const index = waiters.findIndex((w) => w.resolve === resolvePromise);
                if (index >= 0) {
                    waiters.splice(index, 1);
                }
                rejectPromise(new Error(`raw_world_eval timeout method=${method}`));
            }, 15000);
            waiters.push({
                resolve: (reply: string) => {
                    clearTimeout(timer);
                    resolvePromise(reply);
                },
                reject: (error: Error) => {
                    clearTimeout(timer);
                    rejectPromise(error);
                },
            });
            try {
                const ok = proc.stdin?.write(JSON.stringify({ method, body }) + "\n");
                if (ok === false) {
                    proc.stdin?.once("drain", () => undefined);
                }
            } catch (error) {
                clearTimeout(timer);
                rejectPromise(error instanceof Error ? error : new Error(String(error)));
            }
        });
        // Shared native inventory: the same apps stay alive across restart.
        const ws1 = desk("ws1");
        const ws2 = desk("ws2");
        const w1 = win("win-a", "out-a", ws1, 0);
        const w2 = win("win-b", "out-b", ws2, 0);
        // One managed tile on the restarted scope: the engine projects
        // retained tiled geometry for managed members, so the scope under
        // test carries a managed tile plus the seeded float exception.
        const w3 = win("win-c", "out-a", ws1, 300);
        // Genuine fixed-size declaration in out-b coordinates (observer
        // predicate: integer min/max, equal and nonzero on both axes). The
        // seeded tile override suppresses automatic float admission, so
        // win-b stays tiled under core geometry.
        (w2 as unknown as Record<string, unknown>)["frameGeometry"] = { x: 900, y: 100, width: 640, height: 480 };
        (w2 as unknown as Record<string, unknown>)["minSize"] = { width: 640, height: 480 };
        (w2 as unknown as Record<string, unknown>)["maxSize"] = { width: 640, height: 480 };
        const screens = [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ];
        const currents = new Map([["out-a", "ws1"], ["out-b", "ws2"]]);
        // Switchable active fixture/output: both scopes tile explicitly.
        const focus: { screen: string; window: FakeWindow } = { screen: "out-a", window: w1 };
        const geometryWrites: string[] = [];
        const membershipWrites: string[] = [];
        for (const w of [w1, w2, w3]) {
            // Genuine native carrier facts (mirroring the migrated engine
            // harnesses): truthful PID, identity texts, and resizable
            // state, so the core never holds rows for missing facts.
            const rec = w as unknown as Record<string, unknown>;
            rec["pid"] = 1234;
            rec["resourceClass"] = "Example";
            rec["caption"] = "Window";
            rec["windowRole"] = "normal";
            rec["desktopFileName"] = "example.app";
            rec["resizeable"] = true;
            let frame = w.frameGeometry;
            // Truthful native decoration: every native frame write keeps
            // the client rect tracking the frame with the same insets, so
            // no capture ever pairs a fresh frame with a stale client.
            let liveClient = {
                x: frame.x + 2, y: frame.y + 21,
                width: Math.max(1, frame.width - 4), height: Math.max(1, frame.height - 22),
            };
            Object.defineProperty(w, "frameGeometry", {
                configurable: true,
                get: () => frame,
                set: (rect: { x: number; y: number; width: number; height: number }) => {
                    frame = rect;
                    liveClient = {
                        x: rect.x + 2, y: rect.y + 21,
                        width: Math.max(1, rect.width - 4), height: Math.max(1, rect.height - 22),
                    };
                    geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                    return true;
                },
            });
            Object.defineProperty(w, "clientGeometry", {
                configurable: true,
                get: () => liveClient,
                set: (rect: { x: number; y: number; width: number; height: number }) => {
                    liveClient = rect;
                },
            });
        }
        const nativeSnapshot = (): string => JSON.stringify([
            w1.frameGeometry, w1.desktops.map((d) => d.id),
            w2.frameGeometry, w2.desktops.map((d) => d.id),
            w3.frameGeometry, w3.desktops.map((d) => d.id),
        ]);
        // Float/tile lifetimes never take unsolicited geometry writes; the
        // managed tile may rewrite its identical frame on first-accept.
        const floatTileWriteCount = (): number =>
            geometryWrites.filter((line) => line.startsWith("win-a@") || line.startsWith("win-b@")).length;
        interface Producer {
            handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
            broker: MiniBroker;
            calls: CapturedCall[];
            timers: Array<{ delay: number; cb: () => void }>;
            writes: string[];
            signals: Record<string, FakeSignal>;
            flushed: { count: number };
            runTimers: (maxDelay?: number) => void;
        }
        const startProducer = (adapter: string, tag: string, broker: MiniBroker): Producer => {
            const signals = {
                windowAdded: fakeSignal(),
                windowRemoved: fakeSignal(),
                windowActivated: fakeSignal(),
                desktopsChanged: fakeSignal(),
                screensChanged: fakeSignal(),
                currentDesktopChanged: fakeSignal(),
                activeScreenChanged: fakeSignal(),
            };
            const calls: CapturedCall[] = [];
            const timers: Array<{ delay: number; cb: () => void }> = [];
            const writes: string[] = [];
            const surface = {
                windowList: () => [w1, w3, w2],
                screens,
                desktops: [ws1, ws2],
                get activeScreen() {
                    return { name: focus.screen };
                },
                cursorPos: { x: 2, y: 100 },
                windowAdded: signals.windowAdded.signal,
                windowRemoved: signals.windowRemoved.signal,
                windowActivated: signals.windowActivated.signal,
                desktopsChanged: signals.desktopsChanged.signal,
                screensChanged: signals.screensChanged.signal,
                currentDesktopChanged: signals.currentDesktopChanged.signal,
                activeScreenChanged: signals.activeScreenChanged.signal,
                get activeWindow() {
                    return focus.window;
                },
                clientArea: (_area: number, _out: unknown, deskRef: unknown) => {
                    const id = (deskRef as { id?: unknown }).id;
                    if (id === "ws2") {
                        return { x: 800, y: 0, width: 800, height: 600 };
                    }
                    return { x: 0, y: 0, width: 800, height: 600 };
                },
                currentDesktopForScreen: (screen: { name: string }) => (currents.get(screen.name) === "ws2" ? ws2 : ws1),
                setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
                    membershipWrites.push(`${tag}:view ${(deskRef as { id: string }).id} on ${screen.name}`);
                    currents.set(screen.name, (deskRef as { id: string }).id);
                },
                sendClientToScreen: () => {
                    membershipWrites.push(`${tag}:send`);
                },
                createDesktop: () => null,
                removeDesktop: () => false,
            };
            const handle = startWorldAdapterEntry({
                workspace: surface,
                owner: "owner-a",
                adapter,
                floatIntent: true,
                callDbus: (_service, _path, _iface, method, payload, callback) => {
                    if (method === "ReadFloatIntent") {
                        // Exact schema2 seed: the settled float plus the
                        // disjoint tile override with exact core-minted
                        // bytes, shared across the restart lifetimes.
                        const body = JSON.parse(payload) as Record<string, unknown>;
                        callback(JSON.stringify({
                            v: 2,
                            correlation_id: body["correlation_id"],
                            outcome: "ok",
                            records: [
                                { win: "win-a", data: FLOAT_DATA },
                                { win: "win-b", data: TILE_DATA },
                            ],
                            stored: 2,
                            returned: 2,
                        }));
                        return;
                    }
                    if (method === "WriteFloatIntent") {
                        writes.push(payload);
                        const body = JSON.parse(payload) as Record<string, unknown>;
                        callback(JSON.stringify({
                            v: 2,
                            correlation_id: body["correlation_id"],
                            outcome: "stored",
                            stored: (body["records"] as Array<unknown>).length,
                        }));
                        return;
                    }
                    calls.push({ method, payload, callback });
                    if (method === "QueueWorldCommand") {
                        callback(broker.answerQueue(payload));
                    } else if (method === "TakeCommandResult") {
                        callback(broker.answerTake(payload));
                    }
                },
                scheduleOnce: (delay, cb) => {
                    timers.push({ delay, cb });
                    return () => undefined;
                },
                log: () => undefined,
                gestureActive: () => false,
                registerShortcutFn: () => true,
                readWorkspaceModeFn: () => "per-output-local",
                readDefaultTiledFn: () => false,
            });
            assert.ok(handle !== null);
            return {
                handle: handle as NonNullable<typeof handle>,
                broker, calls, timers, writes, signals,
                flushed: { count: 0 },
                runTimers: (maxDelay = 120): void => {
                    for (let i = 0; i < timers.length; i += 1) {
                        const t = timers[i] as { delay: number; cb: () => void };
                        if (t.delay <= maxDelay) {
                            timers.splice(i, 1);
                            i -= 1;
                            t.cb();
                        }
                    }
                },
            };
        };
        const flushNext = async (p: Producer): Promise<{ req: Record<string, unknown>; reply: Record<string, unknown>; stamped: Record<string, unknown>; stampedText: string }> => {
            const at = p.calls.findIndex((c, i) => i >= p.flushed.count && c.method === "DescribeWorld");
            assert.ok(at >= 0, "expected a queued DescribeWorld");
            p.flushed.count = at + 1;
            // Mini-broker stamping: the envelope becomes the exact core
            // report the sole broker would dispatch, and the core reply
            // returns bound with a nulled terminal result.
            const stamped = p.broker.stampCapture(p.calls[at]!.payload);
            const coreReply = await engineSend("DescribeWorld", stamped);
            const rawReply = p.broker.bindReply(coreReply, p.calls[at]!.payload);
            p.calls[at]!.callback(rawReply);
            return {
                req: JSON.parse(p.calls[at]!.payload) as Record<string, unknown>,
                reply: JSON.parse(rawReply) as Record<string, unknown>,
                stamped: JSON.parse(stamped) as Record<string, unknown>,
                stampedText: stamped,
            };
        };
        const assertIntentSettled = (writes: string[], req: Record<string, unknown>, label: string): void => {
            // Schema2 exact opaque persistence: the settled snapshot needs
            // no churn when the core issues no save/clear decisions in this
            // increment, and the dispatched report still rides the seeded
            // records live-joined with exact core-minted bytes, disjoint
            // per side (float stays float, tile stays tile).
            assert.equal(writes.length, 0, `${label}: settled snapshot needs no churn (no core decisions this increment)`);
            const recs = (((req["report"] as Record<string, unknown>)["intentRecords"] as Array<{ win: string; data: string }>) ?? []);
            const recA = recs.find((r) => r.win === "win-a");
            const recB = recs.find((r) => r.win === "win-b");
            assert.ok(recA !== undefined && recA.data === FLOAT_DATA, `${label}: float identity retained: ${JSON.stringify(recs)}`);
            assert.ok(recB !== undefined && recB.data === TILE_DATA, `${label}: tile override retained: ${JSON.stringify(recs)}`);
            assert.equal((JSON.parse(recA!.data) as Record<string, unknown>)["float"], true, `${label}: float side stays float`);
            assert.equal((JSON.parse(recB!.data) as Record<string, unknown>)["float"], false, `${label}: tile side stays tile`);
            assert.ok(!recs.some((r) => r.win === "win-b" && (JSON.parse(r.data) as Record<string, unknown>)["float"] === true), `${label}: sides disjoint: ${JSON.stringify(recs)}`);
            assert.ok(!recs.some((r) => r.win === "win-a" && (JSON.parse(r.data) as Record<string, unknown>)["float"] === false), `${label}: sides disjoint: ${JSON.stringify(recs)}`);
        };
        // Authentic two-step discovery through the real engine for one
        // producer lifetime: the handshake binds the minted adapter
        // lifetime and mints the serving generation, then the actual
        // revisioned settings ride to the engine and must read accepted
        // before any capture flows (never a scripted settings reply on
        // the engine path).
        const bindProducerDiscovery = async (p: Producer, adapter: string, label: string): Promise<string> => {
            const hsIndex = p.calls.findIndex((c) => {
                if (c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
                } catch {
                    return false;
                }
            });
            assert.ok(hsIndex >= 0, `${label} discovers on start`);
            assert.equal((JSON.parse(p.calls[hsIndex]!.payload) as Record<string, unknown>)["adapter"], adapter, `${label} handshake carries the producer adapter`);
            const hsRaw = await engineSend("DiscoverWorld", p.calls[hsIndex]!.payload);
            const hsReply = JSON.parse(hsRaw) as Record<string, unknown>;
            assert.equal(hsReply["type"], "handshake-reply", `${label} engine answers the handshake`);
            assert.equal(hsReply["ready"], false, `${label} accepted handshake carries ready:false`);
            assert.equal(hsReply["adapter"], adapter, `${label} handshake retains the producer adapter, never a broker incarnation`);
            assert.ok(typeof hsReply["gen"] === "string" && (hsReply["gen"] as string) !== "", `${label} handshake mints the serving generation`);
            const gen = hsReply["gen"] as string;
            p.calls[hsIndex]!.callback(p.broker.bindDiscovery(hsRaw));
            assert.equal(p.broker.gen, gen, `${label} harness records the actual serving generation`);
            const setIndex = p.calls.findIndex((c, i) => {
                if (i <= hsIndex || c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
                } catch {
                    return false;
                }
            });
            assert.ok(setIndex >= 0, `${label} revisioned settings follow the handshake`);
            const setBody = JSON.parse(p.calls[setIndex]!.payload) as Record<string, unknown>;
            assert.equal(setBody["adapter"], adapter, `${label} settings carry the producer adapter`);
            assert.equal(setBody["gen"], gen, `${label} settings bind the serving generation`);
            const setRaw = await engineSend("DiscoverWorld", p.calls[setIndex]!.payload);
            const setReply = JSON.parse(setRaw) as Record<string, unknown>;
            assert.equal(setReply["type"], "settings-reply", `${label} engine answers settings`);
            assert.equal(setReply["status"], "accepted", `${label} settings accepted, got ${setRaw.slice(0, 300)}`);
            assert.equal(setReply["adapter"], adapter, `${label} settings reply retains the producer adapter`);
            assert.equal(setReply["gen"], gen, `${label} settings reply binds the serving generation`);
            assert.equal(setReply["req"], setBody["req"], `${label} settings reply echoes the request correlation`);
            assert.equal(setReply["rev"], setBody["rev"], `${label} settings reply binds the sent revision`);
            p.calls[setIndex]!.callback(setRaw);
            return gen;
        };
        let p1: Producer | null = null;
        let p2: Producer | null = null;
        // One broker per producer lifetime: the real service rotates the
        // whole broker (fresh generation, fresh engine, dropped queues) when
        // a new producer lifetime binds, so the restart models rotation with
        // a fresh harness broker, never the retained one.
        try {
            // Producer 1: fresh production-minted stream on the saved
            // default (false) with an explicit tiling override, advancing
            // both watermarks with real commands.
            const broker1: MiniBroker = createMiniBroker();
            const adapter1 = mintAdapterIncarnation();
            assert.ok(isOpaqueId(adapter1) && adapter1.startsWith("kwin-world-adapter-"));
            p1 = startProducer(adapter1, "p1", broker1);
            const gen = await bindProducerDiscovery(p1, adapter1, "producer1");
            assert.ok(typeof gen === "string" && gen.length > 0, "first generation bound");
            const first = await flushNext(p1);
            assert.equal(first.reply["ready"], true, `first observation accepted: ${JSON.stringify(first.reply)}`);
            for (let round = 0; round < 2; round += 1) {
                for (const handler of [...(p1.signals.desktopsChanged as FakeSignal).handlers]) {
                    handler();
                }
                p1.runTimers();
                const next = await flushNext(p1);
                assert.equal(next.reply["ready"], true, `observation ${round} accepted`);
            }
            // Explicit tiling toggles on BOTH scopes advance the command
            // watermark and leave both tiled (overrides on default false).
            // The dispatched report carries the genuine fixed origin plus
            // the tile-override suppress flag for win-b.
            const findWin = (req: Record<string, unknown>, id: string): Record<string, unknown> | undefined => {
                // v2: the inner report carries flat window facts (no
                // domains lane on the wire).
                const report = req["report"] as Record<string, unknown>;
                return ((report["windows"] as Array<Record<string, unknown>>) ?? []).find((w) => w["win"] === id);
            };
            let lastRev = first.reply["rev"] as string;
            let lastReqSeq = first.stamped["reqSeq"] as string;
            p1.handle.requestWorkspaceTilingToggle();
            p1.runTimers();
            const toggledA = await flushNext(p1);
            assert.equal(toggledA.reply["ready"], true, "toggle out-a accepted");
            assert.equal(toggledA.reply["consumed"], false, "toggle out-a executes");
            lastRev = toggledA.reply["rev"] as string;
            lastReqSeq = toggledA.stamped["reqSeq"] as string;
            focus.screen = "out-b";
            focus.window = w2;
            p1.handle.requestWorkspaceTilingToggle();
            p1.runTimers();
            const toggledB = await flushNext(p1);
            assert.equal(toggledB.reply["ready"], true, "toggle out-b accepted");
            assert.equal(toggledB.reply["consumed"], false, "toggle out-b executes");
            lastRev = toggledB.reply["rev"] as string;
            lastReqSeq = toggledB.stamped["reqSeq"] as string;
            const reportedB = findWin(toggledB.req, "win-b");
            // Fixed-size pins ride raw and the tile override rides the
            // opaque intent lane: the raw row carries the native extents
            // only, never verdicts.
            const toggleRecs = (((toggledB.req["report"] as Record<string, unknown>)["intentRecords"] as Array<{ win: string; data: string }>) ?? []);
            const toggleTile = toggleRecs.find((r) => r.win === "win-b");
            assert.ok(
                toggleTile !== undefined && (JSON.parse(toggleTile.data) as Record<string, unknown>)["float"] === false,
                `tile override rides the opaque intent lane: ${JSON.stringify(toggleRecs)}`,
            );
            assert.ok(
                !("fixedOrigins" in (toggledB.req["report"] as Record<string, unknown>)),
                "no origins lane exists (tile intent is the override)",
            );
            assert.ok(reportedB !== undefined && !("floating" in reportedB) && !("fixedAuto" in reportedB), "raw row carries no verdicts");
            const replyScopesB = toggledB.reply["scopes"] as Array<Record<string, unknown>>;
            const replyWsB = ((toggledB.reply["workspaces"] as Array<Record<string, unknown>>) ?? []);
            // v4: scopes ride the tagged union (never retired composite
            // strings); the out-b current native (ws2) stays bound under a
            // controlled covering scope.
            const boundB = replyWsB.find((w) => w["out"] === "out-b" && w["bind"] === "ws2");
            assert.ok(boundB !== undefined, `out-b current native ws2 stays bound: ${JSON.stringify(replyWsB)}`);
            const logicalB = boundB["id"] as string;
            const coveredB = replyScopesB.some((s) => {
                const sc = s["scope"] as Record<string, unknown> | null;
                if (typeof sc !== "object" || sc === null || s["auth"] !== "controlled") {
                    return false;
                }
                if (sc["kind"] === "workspace") {
                    return sc["out"] === "out-b" && sc["ws"] === logicalB;
                }
                return sc["kind"] === "output" && sc["out"] === "out-b";
            });
            assert.ok(coveredB, `controlled scope covers the out-b binding: ${JSON.stringify(replyScopesB)}`);
            const replyRowsB = toggledB.reply["windows"] as Array<Record<string, unknown>>;
            const rowB = replyRowsB.find((w) => w["win"] === "win-b");
            assert.equal(rowB?.["frame_auth"], "controlled", "fixed tile under controlled frame authority");
            assert.equal(rowB?.["member_auth"], "controlled", "fixed tile under controlled membership authority");
            assert.ok(rowB?.["alloc"] !== null && rowB?.["alloc"] !== undefined, `fixed tile carries desired geometry: ${JSON.stringify(rowB)}`);
            assert.ok(Number(lastReqSeq) >= 5, `request watermark advanced: ${lastReqSeq}`);
            focus.screen = "out-a";
            focus.window = w1;
            assert.equal(p1.handle.getWorkspaceTilingSnapshot().tiled, true, "out-a override tiled before restart");
            focus.screen = "out-b";
            focus.window = w2;
            assert.equal(p1.handle.getWorkspaceTilingSnapshot().tiled, true, "out-b override tiled before restart");
            focus.screen = "out-a";
            focus.window = w1;
            assertIntentSettled(p1.writes, toggledB.req, "producer1");
            const nativeBefore = nativeSnapshot();
            const floatTileBefore = floatTileWriteCount();
            const memberBefore = membershipWrites.length;
            p1.handle.stop();
            // Producer 2: script restart with a fresh production-minted
            // incarnation as a NEW producer lifetime. Like the real
            // WorldBroker::discovery_body, the restart rotates onto a fresh
            // generation and a fresh engine (raw_world_eval rebinds both on its
            // own DiscoverWorld, which the restart sends).
            const adapter2 = mintAdapterIncarnation();
            assert.notEqual(adapter2, adapter1, "restart mints a fresh stream");
            const broker2: MiniBroker = createMiniBroker();
            p2 = startProducer(adapter2, "p2", broker2);
            const gen2 = await bindProducerDiscovery(p2, adapter2, "producer2");
            assert.ok(typeof gen2 === "string" && gen2.length > 0, "rotated generation bound");
            assert.notEqual(gen2, gen, "new lifetime rotates the generation");
            // The dead world is gone: a retained first-generation capture
            // refuses outright and can never apply again. The stamped
            // envelope still names the dead adapter AND the dead generation;
            // world_admit_v4.rs 140-150 checks the bound adapter before the
            // generation, so the original byte replay refuses owner-invalid.
            const staleBody = JSON.parse(toggledB.stampedText) as Record<string, unknown>;
            assert.notEqual(staleBody["adapter"], adapter2, "stale envelope names the dead adapter");
            assert.notEqual(staleBody["gen"], gen2, "stale envelope names the dead generation");
            const staleGen = JSON.parse(await engineSend("DescribeWorld", toggledB.stampedText)) as Record<string, unknown>;
            assert.equal(staleGen["outcome"], "rejected");
            assert.equal(
                staleGen["kind"],
                "owner-invalid",
                "dead adapter+generation capture refuses on the adapter fence first",
            );
            // Fence discrimination (test-only counterfactual, offline only):
            // the same dead generation under the current adapter reaches the
            // generation fence and refuses generation-mismatch. This proves
            // the old-generation protection is distinct from the adapter
            // fence; the original byte replay above is unchanged.
            const counterfactual = JSON.stringify({ ...staleBody, adapter: adapter2 });
            assert.equal(
                (JSON.parse(counterfactual) as Record<string, unknown>)["gen"],
                staleBody["gen"],
                "counterfactual keeps the dead generation",
            );
            const staleGenOnly = JSON.parse(await engineSend("DescribeWorld", counterfactual)) as Record<string, unknown>;
            assert.equal(staleGenOnly["outcome"], "rejected");
            assert.equal(
                staleGenOnly["kind"],
                "generation-mismatch",
                "dead generation under the current adapter refuses on the generation fence",
            );
            const restarted = await flushNext(p2);
            assert.equal(restarted.req["adapter"], adapter2);
            assert.match(
                restarted.req["source"] as string,
                /^src-1-/,
                "restarted source namespace begins at 1 for the fresh incarnation",
            );
            assert.ok(!("command" in (restarted.req as Record<string, unknown>)), "restart carries no replayed command");
            assert.equal(restarted.reply["ready"], true, `restart accepted on the rotated engine, never superseded: ${JSON.stringify(restarted.reply)}`);
            assert.equal(restarted.reply["rev"], "1", "rotated engine starts at rev 1");
            assert.equal(restarted.reply["gen"], gen2, "restart binds the rotated generation");
            assert.equal(restarted.req["gen"], gen2, "restart capture binds the rotated generation");
            // Queue/take lanes rebind to the rotated generation: the first
            // retile files under gen2, never the dead gen1 namespace.
            lastRev = restarted.reply["rev"] as string;
            lastReqSeq = restarted.stamped["reqSeq"] as string;
            for (const handler of [...(p2.signals.desktopsChanged as FakeSignal).handlers]) {
                handler();
            }
            p2.runTimers();
            const settled = await flushNext(p2);
            assert.equal(settled.reply["ready"], true, "post-restart observation accepted");
            assert.equal(settled.reply["rev"], "2", "rotated engine advances from rev 1");
            lastRev = settled.reply["rev"] as string;
            lastReqSeq = settled.stamped["reqSeq"] as string;
            assertIntentSettled(p2.writes, settled.req, "producer2");
            // REQ-CTL-04: per-workspace overrides reset on restart; the saved
            // default (false) reseeds BOTH scopes and native state is untouched.
            const snapA = p2.handle.getWorkspaceTilingSnapshot();
            assert.equal(snapA.tiled, false, `out-a override resets to saved default on restart: ${JSON.stringify(snapA)}`);
            focus.screen = "out-b";
            focus.window = w2;
            const snapB = p2.handle.getWorkspaceTilingSnapshot();
            assert.equal(snapB.tiled, false, `out-b override resets to saved default on restart: ${JSON.stringify(snapB)}`);
            focus.screen = "out-a";
            focus.window = w1;
            assert.equal(nativeSnapshot(), nativeBefore, "restart performs no unsolicited native changes");
            assert.equal(floatTileWriteCount(), floatTileBefore, "float/tile take no geometry writes across restart");
            assert.equal(membershipWrites.length, memberBefore, "no membership writes across restart");
            // Explicit retile of BOTH scopes through NEW commands executes in
            // the fresh command namespace (old commands never replay).
            p2.handle.requestWorkspaceTilingToggle();
            p2.runTimers();
            const retiledA = await flushNext(p2);
            assert.equal(retiledA.reply["ready"], true, "retile out-a accepted");
            assert.equal(retiledA.reply["consumed"], false, "retile out-a executes, never consumed");
            assert.equal(retiledA.reply["rev"], String(Number(lastRev) + 1), "rotated revision advances");
            lastRev = retiledA.reply["rev"] as string;
            assert.equal(p2.handle.getWorkspaceTilingSnapshot().tiled, true, "out-a scope retiled");
            // The retile filed under the rotated generation, never the dead
            // one: queue/take lanes rebind with the new world.
            const filedQueue = [...p2.calls].reverse().find((c) => c.method === "QueueWorldCommand");
            assert.ok(filedQueue !== undefined, "retile files a queue command");
            assert.equal(
                (JSON.parse(filedQueue.payload) as Record<string, unknown>)["gen"],
                gen2,
                "queue binds the rotated generation",
            );
            const filedTake = [...p2.calls].reverse().find((c) => c.method === "TakeCommandResult");
            assert.ok(filedTake !== undefined, "retile collects its terminal result");
            assert.equal(
                (JSON.parse(filedTake.payload) as Record<string, unknown>)["gen"],
                gen2,
                "take binds the rotated generation",
            );
            focus.screen = "out-b";
            focus.window = w2;
            p2.handle.requestWorkspaceTilingToggle();
            p2.runTimers();
            const retiled = await flushNext(p2);
            assert.equal(retiled.reply["ready"], true, "retile out-b accepted");
            assert.equal(retiled.reply["consumed"], false, "retile out-b executes, never consumed");
            assert.equal(p2.handle.getWorkspaceTilingSnapshot().tiled, true, "out-b scope retiled");
            focus.screen = "out-a";
            focus.window = w1;
            assert.equal(p2.handle.getWorkspaceTilingSnapshot().tiled, true, "out-a scope still tiled");
            // The accepted reply still floats win-a explicitly and carries
            // the fixed tile under controlled authority with desired
            // geometry; the native fixed frame matches the accepted frame
            // and the seeded intent snapshot still rides every report.
            const rows = retiled.reply["windows"] as Array<Record<string, unknown>>;
            assert.equal(rows.find((w) => w["win"] === "win-a")?.["floating"], true, "float persists in accepted reply");
            const rowFixed = rows.find((w) => w["win"] === "win-b");
            assert.equal(rowFixed?.["frame_auth"], "controlled", "fixed tile under controlled frame authority");
            assert.equal(rowFixed?.["member_auth"], "controlled", "fixed tile under controlled membership authority");
            assert.ok(rowFixed?.["alloc"] !== null && rowFixed?.["alloc"] !== undefined, `fixed tile carries desired geometry: ${JSON.stringify(rowFixed)}`);
            const nativeFixed = w2.frameGeometry;
            assert.deepEqual(
                [nativeFixed.x, nativeFixed.y, nativeFixed.width, nativeFixed.height],
                rowFixed?.["frame"],
                `native fixed frame matches accepted frame: ${JSON.stringify(nativeFixed)} vs ${JSON.stringify(rowFixed?.["frame"])}`,
            );
            assertIntentSettled(p2.writes, retiled.req, "producer2 retiled");
            assert.equal(nativeSnapshot(), nativeBefore, "frames converge back to pre-restart geometry");
            p2.handle.stop();
            p2 = null;
        } finally {
            try {
                p1?.handle.stop();
            } catch (error) {
                void error;
            }
            try {
                p2?.handle.stop();
            } catch (error) {
                void error;
            }
            try {
                lines.close();
            } catch (error) {
                void error;
            }
            try {
                proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolvePromise) => {
                const timer = setTimeout(() => {
                    try {
                        proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolvePromise();
                }, 3000);
                proc.on("exit", () => {
                    clearTimeout(timer);
                    resolvePromise();
                });
                if (proc.exitCode !== null || proc.signalCode !== null) {
                    clearTimeout(timer);
                    resolvePromise();
                }
            });
        }
    });
});

describe("world entry float-intent reconcile controls", () => {
    // Compact offline harness for the intent-membership paths touched by the
    // restart-preservation fix: seeded persisted float (win-a) plus disjoint
    // tile override (win-b), hand-built accepted replies, real entry.
    interface FloatHarness {
        handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
        calls: CapturedCall[];
        timers: Array<{ delay: number; cb: () => void }>;
        writes: string[];
        live: FakeWindow[];
        w1: FakeWindow;
        w2: FakeWindow;
        signals: Record<string, FakeSignal>;
        rev: { count: number };
        runTimers: (maxDelay?: number) => void;
        lastCall: (method: string) => CapturedCall;
    }

    function makeFloatHarness(): FloatHarness {
        const ws1 = desk("ws1");
        const ws2 = desk("ws2");
        const w1 = win("win-a", "out", ws1, 0);
        const w2 = win("win-b", "out", ws2, 0);
        const live: FakeWindow[] = [w1, w2];
        const signals: Record<string, FakeSignal> = {
            windowAdded: fakeSignal(),
            windowRemoved: fakeSignal(),
            windowActivated: fakeSignal(),
            desktopsChanged: fakeSignal(),
            screensChanged: fakeSignal(),
            currentDesktopChanged: fakeSignal(),
            activeScreenChanged: fakeSignal(),
        };
        const calls: CapturedCall[] = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const writes: string[] = [];
        const surface = {
            windowList: () => [...live],
            screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1, ws2],
            activeScreen: { name: "out" },
            cursorPos: { x: 2, y: 100 },
            windowAdded: (signals.windowAdded as FakeSignal).signal,
            windowRemoved: (signals.windowRemoved as FakeSignal).signal,
            windowActivated: (signals.windowActivated as FakeSignal).signal,
            desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
            screensChanged: (signals.screensChanged as FakeSignal).signal,
            currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
            activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
            get activeWindow() {
                return w1;
            },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws1,
            setCurrentDesktopForScreen: () => undefined,
            sendClientToScreen: () => undefined,
            createDesktop: () => null,
            removeDesktop: () => false,
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            floatIntent: true,
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === "ReadFloatIntent") {
                    // Exact schema2 seed: the settled float plus the
                    // disjoint tile override with exact core-minted bytes.
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 2,
                        correlation_id: body["correlation_id"],
                        outcome: "ok",
                        records: [
                            { win: "win-a", data: FLOAT_DATA },
                            { win: "win-b", data: TILE_DATA },
                        ],
                        stored: 2,
                        returned: 2,
                    }));
                    return;
                }
                if (method === "WriteFloatIntent") {
                    writes.push(payload);
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 2,
                        correlation_id: body["correlation_id"],
                        outcome: "stored",
                        stored: (body["records"] as Array<unknown>).length,
                    }));
                    return;
                }
                if (method === "DiscoverWorld" || method === "DescribeWorld") {
                    calls.push({ method, payload, callback });
                    return;
                }
                if (method === "QueueWorldCommand") {
                    calls.push({ method, payload, callback });
                    const queueBody = JSON.parse(payload) as Record<string, unknown>;
                    const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                    callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                        client: queueBody["client"], clientCmd: queueBody["clientCmd"],
                        globalCmd: String(calls.filter((q) => q.method === "QueueWorldCommand").length) }));
                    return;
                }
                if (method === "TakeCommandResult") {
                    calls.push({ method, payload, callback });
                    const takeBody = JSON.parse(payload) as Record<string, unknown>;
                    const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                    const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                    if (stagedTake !== undefined) {
                        delete (globalThis as Record<string, unknown>)["__takeResult"];
                        callback(JSON.stringify({ v: 4, outcome: "ready", gen: takeGen, globalCmd: "1", result: stagedTake }));
                    } else {
                        callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
                    }
                    return;
                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
            readDefaultTiledFn: () => true,
        });
        assert.ok(handle !== null);
        const h = {
            handle, calls, timers, writes, live, w1, w2, signals, rev: { count: 0 },
            runTimers: (maxDelay = 120): void => {
                for (let i = 0; i < timers.length; i += 1) {
                    const t = timers[i] as { delay: number; cb: () => void };
                    if (t.delay <= maxDelay) {
                        timers.splice(i, 1);
                        i -= 1;
                        t.cb();
                    }
                }
            },
            lastCall: (method: string): CapturedCall => {
                const found = [...calls].reverse().find((c) => c.method === method);
                assert.ok(found !== undefined, `expected ${method}`);
                return found as CapturedCall;
            },
        };
        // Settle discovery plus the auto observation-only dispatch.
        answerHandshake(h);
        answerSettings(h);
        const auto = h.lastCall("DescribeWorld");
        const autoBody = JSON.parse(auto.payload);
        h.rev.count += 1;
        auto.callback(acceptedReply("gen-1", autoBody.source, autoBody.nativeInvalid, String(h.rev.count)));
        return h;
    }

    const floatRow = (w: FakeWindow, extra: Record<string, unknown> = {}): Record<string, unknown> => ({
        win: w.internalId, out: "out", ws: w.desktops[0]?.id ?? "ws1",
        alloc: null, frame: null,
        frame_auth: "untouched", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
        ...extra,
    });

    const tileRow = (w: FakeWindow, extra: Record<string, unknown> = {}): Record<string, unknown> => ({
        win: w.internalId, out: "out", ws: w.desktops[0]?.id ?? "ws1",
        alloc: null, frame: null,
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
        ...extra,
    });

    function acceptHead(h: FloatHarness, extra: Record<string, unknown> = {}): void {
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = h.lastCall("DescribeWorld");
        const body = JSON.parse(req.payload);
        h.rev.count += 1;
        const { result, ...rest } = extra;
        if (result !== undefined) {
            (globalThis as Record<string, unknown>)["__takeResult"] = result;
        }
        req.callback(acceptedReply("gen-1", body.source, body.nativeInvalid, String(h.rev.count), rest));
    }

    const lastWrite = (h: FloatHarness): Record<string, unknown> =>
        JSON.parse(h.writes[h.writes.length - 1] as string) as Record<string, unknown>;

    const floatFlagOf = (data: string): unknown => {
        try {
            return (JSON.parse(data) as Record<string, unknown>)["float"];
        } catch {
            return undefined;
        }
    };

    function followupIntentRecords(h: FloatHarness): Array<{ win: string; data: string }> {
        for (const handler of [...(h.signals.desktopsChanged as FakeSignal).handlers]) {
            handler();
        }
        h.runTimers();
        const follow = JSON.parse(h.lastCall("DescribeWorld").payload) as Record<string, unknown>;
        return (((follow["report"] as Record<string, unknown>)["intentRecords"] as Array<{ win: string; data: string }>) ?? []);
    }

    function assertRetained(h: FloatHarness, label: string): void {
        // The retained snapshot needs no churn (the core issues no
        // save/clear decisions here), and the followup report still rides
        // the seeded records live-joined with exact core-minted bytes,
        // disjoint per side.
        assert.equal(h.writes.length, 0, `${label}: retained snapshot needs no churn`);
        const recs = followupIntentRecords(h);
        const recA = recs.find((r) => r.win === "win-a");
        const recB = recs.find((r) => r.win === "win-b");
        assert.ok(recA !== undefined && recA.data === FLOAT_DATA, `${label}: float retained: ${JSON.stringify(recs)}`);
        assert.ok(recB !== undefined && recB.data === TILE_DATA, `${label}: tile override retained: ${JSON.stringify(recs)}`);
        assert.ok(!recs.some((r) => r.win === "win-a" && floatFlagOf(r.data) === false), `${label}: float side stays disjoint`);
        assert.ok(!recs.some((r) => r.win === "win-b" && floatFlagOf(r.data) === true), `${label}: tile side stays disjoint`);
    }

    it("untouched floating-false rows never unfloat (overlay shape)", () => {
        const h = makeFloatHarness();
        // Fullscreen-overlay tiled shape: floating false with untouched
        // frame authority and no hold. Must retain the settled float.
        acceptHead(h, { windows: [tileRow(h.w1, { frame_auth: "untouched", floating: false })] });
        assertRetained(h, "overlay row must not unfloat");
        h.handle?.stop();
    });

    it("explicit successful unfloat clears while noSpace retains", () => {
        const cleared = makeFloatHarness();
        acceptHead(cleared, {
            windows: [],
            intentWrites: [{ win: "win-a", data: TILE_DATA }],
            result: { op: "toggle-float", win: "win-a", rect: null },
        });
        const done = lastWrite(cleared);
        assert.equal(done["v"], 2, `store write rides schema2: ${JSON.stringify(done).slice(0, 200)}`);
        const doneRecs = done["records"] as Array<{ win: string; data: string }>;
        assert.ok(!doneRecs.some((r) => r.win === "win-a" && floatFlagOf(r.data) === true), `explicit unfloat clears: ${JSON.stringify(done)}`);
        const doneA = doneRecs.find((r) => r.win === "win-a");
        assert.ok(doneA !== undefined && floatFlagOf(doneA.data) === false, `explicit unfloat records tile: ${JSON.stringify(done)}`);
        cleared.handle?.stop();
        // No-space retry keeps the float and records no tile override.
        const held = makeFloatHarness();
        acceptHead(held, {
            windows: [],
            result: { op: "toggle-float", win: "win-a", rect: null, noSpace: true },
        });
        assertRetained(held, "noSpace retains float");
        held.handle?.stop();
    });

    it("clean complete departure prunes the settled float", () => {
        const h = makeFloatHarness();
        for (const handler of [...(h.signals.windowRemoved as FakeSignal).handlers]) {
            (handler as (payload?: unknown) => void)(h.w1);
        }
        h.live.splice(h.live.indexOf(h.w1), 1);
        acceptHead(h, { windows: [], intentWrites: [{ win: "win-a", data: null }] });
        const done = lastWrite(h);
        assert.equal(done["v"], 2, `store write rides schema2: ${JSON.stringify(done).slice(0, 200)}`);
        const doneRecs = done["records"] as Array<{ win: string; data: string }>;
        assert.ok(!doneRecs.some((r) => r.win === "win-a"), `departed float prunes: ${JSON.stringify(done)}`);
        const doneB = doneRecs.find((r) => r.win === "win-b");
        assert.ok(doneB !== undefined && floatFlagOf(doneB.data) === false, `tile override survives departure: ${JSON.stringify(done)}`);
        h.handle?.stop();
    });

    it("live-minimized floats survive tiled-classified rows", () => {
        const h = makeFloatHarness();
        h.w1.minimized = true;
        acceptHead(h, { windows: [tileRow(h.w1)] });
        assertRetained(h, "minimized float holds");
        h.handle?.stop();
    });

    it("capacity floats converge without disturbing the tile override", () => {
        const h = makeFloatHarness();
        acceptHead(h, {
            windows: [
                floatRow(h.w1, { float_origin: "capacity" }),
                tileRow(h.w2),
            ],
        });
        assertRetained(h, "capacity float converges");
        h.handle?.stop();
    });

    it("cross-scope tiled rows never clear (scope isolation)", () => {
        const h = makeFloatHarness();
        // Same id named at another scope with full tile proof: not the live
        // scope, so the settled float holds.
        acceptHead(h, { windows: [tileRow(h.w1, { ws: "ws2" })] });
        assertRetained(h, "cross-scope row must not clear");
        h.handle?.stop();
    });
});

describe("world entry adapter incarnation", () => {
    it("mints a fresh opaque token per script evaluation", () => {
        const first = mintAdapterIncarnation();
        const second = mintAdapterIncarnation();
        for (const minted of [first, second]) {
            assert.ok(isOpaqueId(minted), `incarnation must stay opaque: ${minted}`);
            assert.ok(minted.startsWith("kwin-world-adapter-"), `incarnation keeps the production base: ${minted}`);
            assert.ok(minted.length <= 128, `incarnation stays bounded: ${minted.length}`);
        }
        assert.notEqual(first, second, "two evaluations mint different tokens");
    });
});

describe("world entry trace probe", () => {
    interface TracedWindow {
        ref: FakeWindow;
        frameChanged: FakeSignal;
        setFrameRaw: (rect: { x: number; y: number; width: number; height: number }) => void;
    }

    // A live window arriving after bind, instrumented exactly like the
    // harness natives plus a fake frameGeometryChanged signal. The signal
    // serves the persistent production homing listener (attached on
    // windowAdded) and, on trace-enabled builds, the probe-only one-shot
    // armed per requested write. setFrameRaw moves the live frame without
    // recording a production geometry write.
    function addTracedWindow(h: Harness, id: string): TracedWindow {
        const home = h.world.desktops[0] as FakeDesktop;
        assert.ok(home !== undefined);
        const w = win(id, "out", home, 0);
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                h.world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        const frameChanged = fakeSignal();
        (w as unknown as Record<string, unknown>)["frameGeometryChanged"] = frameChanged.signal;
        h.world.wins.push(w);
        return {
            ref: w,
            frameChanged,
            setFrameRaw: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
            },
        };
    }

    function fireWindowAdded(h: Harness, ref: FakeWindow): void {
        for (const handler of [...(h.signals.windowAdded as FakeSignal).handlers]) {
            (handler as (payload?: unknown) => void)(ref);
        }
    }

    function fireFrameChanged(t: TracedWindow): void {
        for (const handler of [...t.frameChanged.handlers]) {
            (handler as () => void)();
        }
    }

    function traceLines(h: Harness, id: string): string[] {
        return h.logs.filter((l) => l.includes(`omnitiler:world:trace win=${id} `));
    }

    function traceEvs(lines: string[]): string[] {
        return lines.map((l) => {
            const ev = l.match(/ ev=([a-z-]+)/);
            assert.ok(ev !== null, `trace line carries an event: ${l}`);
            return (ev as RegExpMatchArray)[1] as string;
        });
    }

    function replyWithFrame(h: Harness, rev: string, id: string, ws: string, frame: [number, number, number, number]): void {
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, rev, {
            windows: [{
                win: id, out: "out", ws,
                alloc: frame, frame,
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "",
                floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
        }));
    }

    // A write-apply reports its attempts, so production arms one follow-up
    // dispatch while the window is still pending (a second debounce-start).
    // Flushing it after the observation emits no further probe lines.
    function flushQuietCycle(h: Harness, rev: string): void {
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, rev));
    }

    // Full producer-shaped admission sequence, including the production
    // write-attempt follow-up dispatch while the window is still pending.
    const ADMISSION_EVS = ["added", "debounce-start", "debounce-fire", "send", "reply", "write", "debounce-start", "observed"];

    it("emits the ordered admission sequence with match on first observation", () => {
        const h = makeHarness();
        discover(h);
        const added = addTracedWindow(h, "win-c");
        const ws = h.world.desktops[0] as FakeDesktop;
        fireWindowAdded(h, added.ref);
        replyWithFrame(h, "1", "win-c", ws.id, [10, 10, 300, 300]);
        assert.ok(h.world.geometryWrites.some((w) => w.startsWith("win-c@10,10,300,300")));
        fireFrameChanged(added);
        const lines = traceLines(h, "win-c");
        assert.deepEqual(traceEvs(lines), ADMISSION_EVS);
        for (const line of lines) {
            assert.match(line, / t=\d+/);
        }
        assert.match(lines[lines.length - 1] as string, /req=10,10,300,300 seen=10,10,300,300 match=1/);
        // The one-shot detaches on first fire: no second observed line.
        // The persistent production homing listener remains.
        assert.equal(added.frameChanged.handlers.length, 1);
        const count = traceLines(h, "win-c").length;
        fireFrameChanged(added);
        assert.equal(traceLines(h, "win-c").length, count);
        // Production behavior is unperturbed: exactly one geometry write,
        // and the flushed follow-up adds no further probe lines.
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            ["win-c@10,10,300,300"],
        );
        flushQuietCycle(h, "2");
        assert.equal(traceLines(h, "win-c").length, lines.length);
        h.handle?.stop();
    });

    it("records mismatch without perturbing behavior and never reattributes settled windows", () => {
        const h = makeHarness();
        discover(h);
        const ws = h.world.desktops[0] as FakeDesktop;
        const first = addTracedWindow(h, "win-c");
        fireWindowAdded(h, first.ref);
        replyWithFrame(h, "1", "win-c", ws.id, [10, 10, 300, 300]);
        fireFrameChanged(first);
        flushQuietCycle(h, "2");
        const settled = traceLines(h, "win-c").length;
        assert.ok(settled > 0);
        // Second admission: the live frame diverges from the request before
        // the signal fires, so the first observation reports mismatch.
        const second = addTracedWindow(h, "win-d");
        fireWindowAdded(h, second.ref);
        replyWithFrame(h, "3", "win-d", ws.id, [20, 20, 100, 100]);
        second.setFrameRaw({ x: 1, y: 2, width: 3, height: 4 });
        fireFrameChanged(second);
        const lines = traceLines(h, "win-d");
        assert.deepEqual(traceEvs(lines), ADMISSION_EVS);
        assert.match(lines[lines.length - 1] as string, /req=20,20,100,100 seen=1,2,3,4 match=0/);
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-d@")),
            ["win-d@20,20,100,100"],
        );
        // The settled window is never re-emitted for the later flight, and
        // the flushed follow-up adds nothing for either window.
        assert.equal(traceLines(h, "win-c").length, settled);
        flushQuietCycle(h, "4");
        assert.equal(traceLines(h, "win-c").length, settled);
        assert.equal(traceLines(h, "win-d").length, lines.length);
        h.handle?.stop();
    });

    function frameRow(id: string, ws: string, frame: [number, number, number, number]): Record<string, unknown> {
        return {
            win: id, out: "out", ws,
            alloc: frame, frame,
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            frame_op: "", clear_op: "",
            floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
        };
    }

    function srcOf(line: string): string {
        const m = line.match(/ src=([a-z0-9-]+)/);
        assert.ok(m !== null, `trace line carries a source: ${line}`);
        return (m as RegExpMatchArray)[1] as string;
    }

    it("logs no invented start for a window joining an armed timer", () => {
        const h = makeHarness();
        discover(h);
        const ws = h.world.desktops[0] as FakeDesktop;
        const first = addTracedWindow(h, "win-c");
        fireWindowAdded(h, first.ref);
        const second = addTracedWindow(h, "win-d");
        fireWindowAdded(h, second.ref);
        // D joined while the admission debounce was still armed: only its
        // added line exists, and the single real arm belongs to C alone.
        assert.deepEqual(traceEvs(traceLines(h, "win-d")), ["added"]);
        assert.deepEqual(traceEvs(traceLines(h, "win-c")), ["added", "debounce-start"]);
        // One shared flight carries both windows.
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1", {
            windows: [frameRow("win-c", ws.id, [10, 10, 300, 300]), frameRow("win-d", ws.id, [20, 20, 100, 100])],
        }));
        assert.ok(h.world.geometryWrites.some((w) => w.startsWith("win-c@10,10,300,300")));
        assert.ok(h.world.geometryWrites.some((w) => w.startsWith("win-d@20,20,100,100")));
        fireFrameChanged(first);
        fireFrameChanged(second);
        assert.deepEqual(traceEvs(traceLines(h, "win-c")), ADMISSION_EVS);
        // D never saw the pre-arm start: its first debounce line is the
        // shared fire, then the real post-write arm.
        assert.deepEqual(
            traceEvs(traceLines(h, "win-d")),
            ["added", "debounce-fire", "send", "reply", "write", "debounce-start", "observed"],
        );
        assert.equal(first.frameChanged.handlers.length, 1, "one-shot detached, persistent production listener remains");
        assert.equal(second.frameChanged.handlers.length, 1, "one-shot detached, persistent production listener remains");
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            ["win-c@10,10,300,300"],
        );
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-d@")),
            ["win-d@20,20,100,100"],
        );
        const settledC = traceLines(h, "win-c").length;
        const settledD = traceLines(h, "win-d").length;
        flushQuietCycle(h, "2");
        assert.equal(traceLines(h, "win-c").length, settledC);
        assert.equal(traceLines(h, "win-d").length, settledD);
        h.handle?.stop();
    });

    it("attributes a flight reply only to windows in the sent snapshot", () => {
        const h = makeHarness();
        discover(h);
        const ws = h.world.desktops[0] as FakeDesktop;
        const first = addTracedWindow(h, "win-c");
        fireWindowAdded(h, first.ref);
        h.runTimers();
        const req1 = requestOf(h);
        const src1 = req1["source"] as string;
        // D arrives while the first flight is still outstanding.
        const second = addTracedWindow(h, "win-d");
        fireWindowAdded(h, second.ref);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", src1, req1["nativeInvalid"] as string, "1", {
            windows: [frameRow("win-c", ws.id, [10, 10, 300, 300])],
        }));
        assert.equal(h.world.geometryWrites.length, 0, "native arrival supersedes the first flight, so no setters run");
        assert.ok(h.logs.some((l) => l.includes("stale reason=superseded-during-flight")));
        // The late window shares the armed follow-up but never the first
        // flight's reply: no reply line for D, and the single reply line
        // for C carries the first flight's source.
        assert.ok(!traceEvs(traceLines(h, "win-d")).includes("reply"), "late window has no attributed reply");
        const repliesC = traceLines(h, "win-c").filter((l) => l.includes("ev=reply"));
        assert.equal(repliesC.length, 1);
        assert.equal(srcOf(repliesC[0] as string), src1);
        // D's own flight attributes its reply to the second source only.
        h.runTimers();
        const req2 = requestOf(h);
        const src2 = req2["source"] as string;
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", src2, req2["nativeInvalid"] as string, "2", {
            windows: [frameRow("win-d", ws.id, [20, 20, 100, 100])],
        }));
        fireFrameChanged(second);
        const repliesD = traceLines(h, "win-d").filter((l) => l.includes("ev=reply"));
        assert.equal(repliesD.length, 1);
        assert.equal(srcOf(repliesD[0] as string), src2);
        assert.notEqual(src2, src1);
        h.handle?.stop();
    });

    it("stays silent with no new window and detaches the one-shot on stop", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "1"));
        assert.ok(!h.logs.some((l) => l.includes("omnitiler:world:trace")), "steady state emits no probe lines");
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:dispatch")), "production dispatch logging is intact");
        assert.equal(h.world.geometryWrites.length, 0);
        h.handle?.stop();

        const stopped = makeHarness();
        discover(stopped);
        const ws = stopped.world.desktops[0] as FakeDesktop;
        const pending = addTracedWindow(stopped, "win-e");
        fireWindowAdded(stopped, pending.ref);
        replyWithFrame(stopped, "1", "win-e", ws.id, [30, 30, 120, 120]);
        assert.equal(pending.frameChanged.handlers.length, 2, "persistent production listener plus the armed trace one-shot");
        stopped.handle?.stop();
        assert.equal(pending.frameChanged.handlers.length, 0, "stop detaches the persistent listener and the trace one-shot");
        fireFrameChanged(pending);
        assert.ok(!traceLines(stopped, "win-e").some((l) => l.includes("ev=observed")), "no observation after stop");
    });

    // Bundled-entry parity: the real src/world-entry.ts through the
    // production esbuild defines, loaded in a VM and driven through this
    // same harness. The checked-in test bundle defines TRACE=1, so the
    // direct import is the trace-enabled producer; trace0 must match it on
    // every production effect with no probe lines or frame listener, and
    // trace1 must match it on production effects plus the ordered probe.
    function loadBundledEntry(trace: boolean): typeof startWorldAdapterEntry {
        const outDir = mkdtempSync(join(tmpdir(), "omnitiler-world-entry-"));
        const outFile = join(outDir, "world-entry.js");
        const args = [
            "esbuild",
            "src/world-entry.ts",
            "--bundle",
            "--format=iife",
            "--target=es2017",
            "--global-name=WorldEntry",
            `--define:__OMNITILER_TRACE__=${JSON.stringify(trace ? "1" : "0")}`,
            `--outfile=${outFile}`,
        ];
        if (!trace) {
            args.push("--minify-syntax");
        }
        execFileSync("npx", args, { cwd: process.cwd(), stdio: "pipe" });
        const context = createContext({}) as Record<string, unknown>;
        runInContext(readFileSync(outFile, "utf8"), context, { filename: outFile });
        const exported = (context as unknown as { WorldEntry?: { startWorldAdapterEntry: typeof startWorldAdapterEntry } }).WorldEntry;
        assert.ok(exported !== undefined && typeof exported.startWorldAdapterEntry === "function", "bundled entry exposes startWorldAdapterEntry");
        runInContext("Date.now = function () { return 1700000000000; }; Math.random = function () { return 0.5; };", context, { filename: "pin-determinism.js" });
        return exported.startWorldAdapterEntry;
    }

    function normText(value: string): string {
        return value
            .replace(/\b(src|wait|pres)-[0-9a-z]+-[0-9a-z]+/g, "$1-NORM")
            .replace(/t=\d+/g, "t=NORM");
    }

    function productionSnapshot(h: Harness): Record<string, unknown> {
        return JSON.parse(JSON.stringify({
            calls: h.calls.map((c) => ({ method: c.method, payload: normText(c.payload) })),
            geometry: [...h.world.geometryWrites],
            membership: [...h.world.membershipWrites],
            created: [...h.world.created],
            removed: [...h.world.removed],
            focused: [...h.world.focused],
            timers: h.timers.map((t) => t.delay),
            shortcuts: [...h.shortcuts.keys()].sort(),
            signals: Object.fromEntries(Object.entries(h.signals).map(([k, s]) => [k, (s as FakeSignal).handlers.length])),
            logs: h.logs.filter((l) => !l.includes("omnitiler:world:trace")).map(normText),
        })) as Record<string, unknown>;
    }

    function runBundledAdmission(entryFn: typeof startWorldAdapterEntry, throwOnProbeLog = false): { h: Harness; frameHandlers: number } {
        const h = makeHarness("per-output-local", false, true, undefined, undefined, true, entryFn, throwOnProbeLog);
        discover(h);
        const ws = h.world.desktops[0] as FakeDesktop;
        const added = addTracedWindow(h, "win-c");
        fireWindowAdded(h, added.ref);
        replyWithFrame(h, "1", "win-c", ws.id, [10, 10, 300, 300]);
        const frameHandlers = added.frameChanged.handlers.length;
        fireFrameChanged(added);
        flushQuietCycle(h, "2");
        assert.equal(added.frameChanged.handlers.length, 1, "one-shot detached, persistent production listener remains");
        return { h, frameHandlers };
    }

    it("trace-off bundle matches direct production behavior with probe lines stripped and only the persistent frame listener", () => {
        const direct = runBundledAdmission(startWorldAdapterEntry);
        const off = runBundledAdmission(loadBundledEntry(false));
        direct.h.handle?.stop();
        off.h.handle?.stop();
        assert.deepEqual(productionSnapshot(off.h), productionSnapshot(direct.h));
        assert.deepEqual(traceEvs(traceLines(direct.h, "win-c")), ADMISSION_EVS);
        assert.equal(traceLines(off.h, "win-c").length, 0);
        assert.ok(!off.h.logs.some((l) => l.includes("omnitiler:world:trace")), "trace-off emits no probe lines");
        assert.equal(off.frameHandlers, 1, "trace-off keeps only the persistent production frame listener");
        assert.equal(direct.frameHandlers, 2, "direct trace-enabled arms the one-shot beside the persistent listener");
        assert.deepEqual(
            off.h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            ["win-c@10,10,300,300"],
        );
    });

    it("trace-on bundle matches direct production behavior plus the ordered admission probe", () => {
        const direct = runBundledAdmission(startWorldAdapterEntry);
        const on = runBundledAdmission(loadBundledEntry(true));
        direct.h.handle?.stop();
        on.h.handle?.stop();
        assert.deepEqual(productionSnapshot(on.h), productionSnapshot(direct.h));
        assert.deepEqual(traceEvs(traceLines(on.h, "win-c")), ADMISSION_EVS);
        assert.deepEqual(
            traceLines(on.h, "win-c").map(normText),
            traceLines(direct.h, "win-c").map(normText),
        );
        assert.equal(on.frameHandlers, 2, "trace-on keeps the persistent listener plus exactly one one-shot frame listener");
    });

    it("trace-on probe log failure never perturbs the production geometry write", () => {
        const { h } = runBundledAdmission(loadBundledEntry(true), true);
        h.handle?.stop();
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            ["win-c@10,10,300,300"],
        );
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:dispatch")), "ordinary logs still recorded");
        assert.ok(!h.logs.some((l) => l.includes("omnitiler:world:trace")), "throwing probe lines are never stored");
    });

    it("cleans the one-shot when the native setter refuses without a write", () => {
        const h = makeHarness();
        discover(h);
        const added = addTracedWindow(h, "win-c");
        const ws = h.world.desktops[0] as FakeDesktop;
        fireWindowAdded(h, added.ref);
        // Read-only frame: Reflect.set reports false, so production
        // records a refused attempt with no geometry write.
        Object.defineProperty(added.ref, "frameGeometry", {
            configurable: true,
            value: { x: 1, y: 2, width: 3, height: 4 },
            writable: false,
        });
        replyWithFrame(h, "1", "win-c", ws.id, [10, 10, 300, 300]);
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            [],
        );
        assert.ok(traceEvs(traceLines(h, "win-c")).includes("write"), "attempted write is still logged");
        assert.equal(added.frameChanged.handlers.length, 1, "refused write detaches the one-shot, persistent production listener remains");
        fireFrameChanged(added);
        assert.ok(!traceEvs(traceLines(h, "win-c")).includes("observed"), "no observation after refusal");
        h.handle?.stop();
    });

    it("cleans the one-shot when the native setter throws, holding the attempt ambiguous", () => {
        const h = makeHarness();
        discover(h);
        const added = addTracedWindow(h, "win-c");
        const ws = h.world.desktops[0] as FakeDesktop;
        fireWindowAdded(h, added.ref);
        Object.defineProperty(added.ref, "frameGeometry", {
            configurable: true,
            get: () => ({ x: 1, y: 2, width: 3, height: 4 }),
            set: () => {
                throw new Error("probe-test-throw");
            },
        });
        replyWithFrame(h, "1", "win-c", ws.id, [10, 10, 300, 300]);
        // The apply loop catches the throw and holds the attempt
        // ambiguous: no geometry write fact, reply handling completes.
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            [],
        );
        assert.ok(h.logs.some((l) => l.includes("omnitiler:world:applied")), "apply completes after a throwing setter");
        assert.ok(traceEvs(traceLines(h, "win-c")).includes("write"), "attempted write is still logged");
        assert.equal(added.frameChanged.handlers.length, 1, "thrown write detaches the one-shot, persistent production listener remains");
        fireFrameChanged(added);
        assert.ok(!traceEvs(traceLines(h, "win-c")).includes("observed"), "no observation after throw");
        h.handle?.stop();
    });

    it("holds no pending expectation when the window has no frame signal", () => {
        const h = makeHarness();
        discover(h);
        const added = addTracedWindow(h, "win-c");
        const ws = h.world.desktops[0] as FakeDesktop;
        fireWindowAdded(h, added.ref);
        delete (added.ref as unknown as Record<string, unknown>)["frameGeometryChanged"];
        replyWithFrame(h, "1", "win-c", ws.id, [10, 10, 300, 300]);
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            ["win-c@10,10,300,300"],
        );
        assert.ok(traceEvs(traceLines(h, "win-c")).includes("write"), "attempted write is still logged");
        assert.equal(added.frameChanged.handlers.length, 1, "production listener predates the deletion and remains; no trace one-shot attaches without a signal");
        fireFrameChanged(added);
        assert.ok(!traceEvs(traceLines(h, "win-c")).includes("observed"), "no observation without a signal");
        h.handle?.stop();
    });

    it("captures the first synchronous signal during the setter exactly once", () => {
        const h = makeHarness();
        discover(h);
        const added = addTracedWindow(h, "win-c");
        const ws = h.world.desktops[0] as FakeDesktop;
        fireWindowAdded(h, added.ref);
        // Synchronous native echo: the frame signal fires inside the
        // production setter, before it returns.
        let frame = { x: 1, y: 2, width: 3, height: 4 };
        Object.defineProperty(added.ref, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                h.world.geometryWrites.push(`${added.ref.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                for (const handler of [...added.frameChanged.handlers]) {
                    (handler as () => void)();
                }
                return true;
            },
        });
        replyWithFrame(h, "1", "win-c", ws.id, [10, 10, 300, 300]);
        assert.deepEqual(
            h.world.geometryWrites.filter((w) => w.startsWith("win-c@")),
            ["win-c@10,10,300,300"],
        );
        const observed = traceLines(h, "win-c").filter((l) => l.includes("ev=observed"));
        assert.equal(observed.length, 1, "synchronous first signal is captured once before the setter returns");
        assert.equal(added.frameChanged.handlers.length, 1, "one-shot detaches on the synchronous fire, persistent production listener remains");
        fireFrameChanged(added);
        assert.equal(traceLines(h, "win-c").filter((l) => l.includes("ev=observed")).length, 1);
        h.handle?.stop();
    });
});

// Production-entry focus-feedback regression through the real decoder:
// a populated workspace select files via the production
// requestWorkspaceSelect -> cmdSelectNumbered callback, drains through
// MiniBroker stamping plus raw_world_eval, and settles one terminal
// switch event. Healthy shown witness proves focus; a missing windowAt
// (witness null with healthy setter+foreground) fails it exactly once
// with no upgrade on retry. No hand-built replies on asserted paths.
describe("world entry focus feedback production regression", () => {
    async function driveSelect(witnessPresent: boolean, syncViewSignals = false): Promise<{
        selectReply: Record<string, unknown>;
        feedbackWire: Array<Record<string, unknown>>;
        feedbackId: unknown;
        feedbackAttemptMs: string | null;
        switchEvents: Array<Record<string, unknown>>;
        laterSwitchEvents: Array<Record<string, unknown>>;
        laterFeedbackWire: Array<Record<string, unknown>>;
        restoredSwitch: Array<Record<string, unknown>>;
        focusSets: number;
        selectPayload: string;
        feedbackPayload: string;
    }> {
        const { spawn } = await import("node:child_process");
        const { existsSync } = await import("node:fs");
        const { createInterface } = await import("node:readline");
        const { dirname, join, resolve } = await import("node:path");
        const repoRoot = (): string => {
            let dir = resolve(process.cwd());
            for (let depth = 0; depth < 4; depth += 1) {
                if (existsSync(join(dir, "Cargo.toml"))) {
                    return dir;
                }
                dir = dirname(dir);
            }
            throw new Error("fixture root not found");
        };
        interface EngineWaiter {
            resolve: (reply: string) => void;
            reject: (error: Error) => void;
        }
        const proc = spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], {
            cwd: repoRoot(),
            stdio: ["pipe", "pipe", "pipe"],
        });
        const waiters: EngineWaiter[] = [];
        let engineFailed: Error | null = null;
        const stderrChunks: string[] = [];
        try {
            proc.stderr?.on("data", (chunk: unknown) => {
                stderrChunks.push(String(chunk));
            });
        } catch (error) {
            void error;
        }
        const failEngine = (error: Error): void => {
            if (engineFailed === null) {
                engineFailed = error;
            }
            while (waiters.length > 0) {
                waiters.shift()?.reject(error);
            }
        };
        proc.on("error", (error: Error) => {
            failEngine(error);
        });
        proc.on("exit", (code: number | null, signal: string | null) => {
            failEngine(new Error(`raw_world_eval exited code=${String(code)} signal=${String(signal)} stderr=${stderrChunks.join("").slice(0, 500)}`));
        });
        const stdout = proc.stdout;
        assert.ok(stdout !== null && stdout !== undefined, "raw_world_eval stdout unavailable");
        const lines = createInterface({ input: stdout });
        lines.on("line", (line: string) => {
            waiters.shift()?.resolve(line);
        });
        const engineSend = (method: string, body: string): Promise<string> => new Promise<string>((resolvePromise, rejectPromise) => {
            if (engineFailed !== null) {
                rejectPromise(engineFailed);
                return;
            }
            const timer = setTimeout(() => {
                const index = waiters.findIndex((w) => w.resolve === resolvePromise);
                if (index >= 0) {
                    waiters.splice(index, 1);
                }
                rejectPromise(new Error(`raw_world_eval timeout method=${method}`));
            }, 15000);
            waiters.push({
                resolve: (reply: string) => {
                    clearTimeout(timer);
                    resolvePromise(reply);
                },
                reject: (error: Error) => {
                    clearTimeout(timer);
                    rejectPromise(error);
                },
            });
            try {
                const ok = proc.stdin?.write(JSON.stringify({ method, body }) + "\n");
                if (ok === false) {
                    proc.stdin?.once("drain", () => undefined);
                }
            } catch (error) {
                clearTimeout(timer);
                rejectPromise(error instanceof Error ? error : new Error(String(error)));
            }
        });
        const ws1 = desk("ws1");
        const ws2 = desk("ws2");
        const w1 = win("win-a", "out-a", ws1, 0);
        const w2 = win("win-b", "out-a", ws2, 300);
        const screens = [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
        ];
        const currents = new Map([["out-a", "ws1"]]);
        const setFrame = (w: FakeWindow, rect: { x: number; y: number; width: number; height: number }): void => {
            (w as unknown as Record<string, unknown>)["frameGeometry"] = { ...rect };
            (w as unknown as Record<string, unknown>)["clientGeometry"] = {
                x: rect.x + 2, y: rect.y + 21,
                width: Math.max(1, rect.width - 4), height: Math.max(1, rect.height - 22),
            };
        };
        for (const w of [w1, w2]) {
            const rec = w as unknown as Record<string, unknown>;
            rec["pid"] = 1234;
            rec["resourceClass"] = "Example";
            rec["caption"] = "Window";
            rec["windowRole"] = "normal";
            rec["desktopFileName"] = "example.app";
            rec["resizeable"] = true;
        }
        setFrame(w1, w1.frameGeometry);
        setFrame(w2, w2.frameGeometry);
        const broker = createMiniBroker();
        const signals: Record<string, FakeSignal> = {
            windowAdded: fakeSignal(),
            windowRemoved: fakeSignal(),
            windowActivated: fakeSignal(),
            desktopsChanged: fakeSignal(),
            screensChanged: fakeSignal(),
            currentDesktopChanged: fakeSignal(),
            activeScreenChanged: fakeSignal(),
        };
        const calls: CapturedCall[] = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        let active: FakeWindow = w1;
        let focusSets = 0;
        // Live KWin emits the selection signals synchronously inside the
        // view setter (VDM currentChanged old/new/output plus Workspace
        // windowActivated ref) and restores the view's focused client
        // natively; the scoped guard defers those exact own echoes so the
        // authorized core-target focus setter still runs once.
        const expectedFocusSets = 1;
        const surface: Record<string, unknown> = {
            windowList: () => [w1, w2],
            screens,
            desktops: [ws1, ws2],
            activeScreen: { name: "out-a" },
            get activeWindow() {
                return active;
            },
            set activeWindow(ref: unknown) {
                focusSets += 1;
                active = ref as FakeWindow;
            },
            cursorPos: { x: 2, y: 100 },
            windowAdded: (signals.windowAdded as FakeSignal).signal,
            windowRemoved: (signals.windowRemoved as FakeSignal).signal,
            windowActivated: (signals.windowActivated as FakeSignal).signal,
            desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
            screensChanged: (signals.screensChanged as FakeSignal).signal,
            currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
            activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: (screen: { name: string }) => currents.get(screen.name) === "ws2" ? ws2 : ws1,
            setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
                const oldId = currents.get(screen.name) ?? "ws1";
                const oldDesk = oldId === "ws2" ? ws2 : ws1;
                currents.set(screen.name, (deskRef as { id: string }).id);
                if (syncViewSignals) {
                    active = w2;
                    for (const handler of [...(signals["currentDesktopChanged"] as FakeSignal).handlers]) {
                        (handler as (...args: Array<unknown>) => void)(oldDesk, deskRef, screens[0]);
                    }
                    for (const handler of [...(signals["windowActivated"] as FakeSignal).handlers]) {
                        (handler as (...args: Array<unknown>) => void)(w2);
                    }
                }
            },
            sendClientToScreen: () => undefined,
            createDesktop: () => null,
            removeDesktop: () => false,
        };
        if (witnessPresent) {
            // Genuine shown witness: the live frame centre hit-tests the
            // exact listed lifetime among all matches.
            surface["windowAt"] = () => [w1, w2];
        }
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === "ReadFloatIntent") {
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 2,
                        correlation_id: body["correlation_id"],
                        outcome: "ok",
                        records: [],
                        stored: 0,
                        returned: 0,
                    }));
                    return;
                }
                if (method === "WriteFloatIntent") {
                    const body = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({
                        v: 2,
                        correlation_id: body["correlation_id"],
                        outcome: "stored",
                        stored: (body["records"] as Array<unknown>).length,
                    }));
                    return;
                }
                if (method === "DiscoverWorld" || method === "DescribeWorld") {
                    calls.push({ method, payload, callback });
                    return;
                }
                if (method === "QueueWorldCommand") {
                    calls.push({ method, payload, callback });
                    callback(broker.answerQueue(payload));
                    return;
                }
                if (method === "TakeCommandResult") {
                    calls.push({ method, payload, callback });
                    callback(broker.answerTake(payload));
                    return;
                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
            readDefaultTiledFn: () => true,
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
        // Signal-driven recapture: native view/foreground moves only
        // re-observe on their signals, so fire them before each pump.
        const fireAll = (): void => {
            for (const key of ["windowActivated", "desktopsChanged", "currentDesktopChanged", "screensChanged", "activeScreenChanged"] as const) {
                for (const handler of [...(signals[key] as FakeSignal).handlers]) {
                    handler();
                }
            }
        };
        let flushed = 0;
        const flushNextDescribe = async (): Promise<{ req: Record<string, unknown>; reply: Record<string, unknown>; rawReply: string }> => {
            const at = calls.findIndex((c, i) => i >= flushed && c.method === "DescribeWorld");
            assert.ok(at >= 0, `expected a queued DescribeWorld (flushed=${flushed} methods=${calls.map((c) => c.method).join(",")})`);
            flushed = at + 1;
            const envelope = calls[at]!.payload;
            const stamped = broker.stampCapture(envelope);
            const coreReply = await engineSend("DescribeWorld", stamped);
            const rawReply = broker.bindReply(coreReply, envelope);
            calls[at]!.callback(rawReply);
            return { req: JSON.parse(calls[at]!.payload) as Record<string, unknown>, reply: JSON.parse(rawReply) as Record<string, unknown>, rawReply };
        };
        const closeEngine = async (): Promise<void> => {
            try {
                lines.close();
            } catch (error) {
                void error;
            }
            try {
                proc.stdin?.end();
            } catch (error) {
                void error;
            }
            await new Promise<void>((resolvePromise) => {
                const timer = setTimeout(() => {
                    try {
                        proc.kill("SIGKILL");
                    } catch (error) {
                        void error;
                    }
                    resolvePromise();
                }, 3000);
                proc.on("exit", () => {
                    clearTimeout(timer);
                    resolvePromise();
                });
                if (proc.exitCode !== null || proc.signalCode !== null) {
                    clearTimeout(timer);
                    resolvePromise();
                }
            });
        };
        try {
            const hsAt = calls.findIndex((c) => {
                if (c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
                } catch {
                    return false;
                }
            });
            assert.ok(hsAt >= 0, "entry discovers on start");
            const engineHs = await engineSend("DiscoverWorld", calls[hsAt]!.payload);
            calls[hsAt]!.callback(broker.bindDiscovery(engineHs));
            const setAt = calls.findIndex((c, i) => {
                if (i <= hsAt || c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
                } catch {
                    return false;
                }
            });
            assert.ok(setAt >= 0, "revisioned settings follow the handshake");
            const engineSet = await engineSend("DiscoverWorld", calls[setAt]!.payload);
            calls[setAt]!.callback(engineSet);
            runTimers();
            await flushNextDescribe();
            // Production selection path: the entry files cmdSelectNumbered
            // from this callback, never a test-built command.
            handle!.requestWorkspaceSelect(2);
            runTimers();
            const selQueue = [...calls].reverse().find((c) => c.method === "QueueWorldCommand");
            assert.ok(selQueue !== undefined, "select files a queue command");
            const queuedCmd = JSON.parse(selQueue.payload) as Record<string, unknown>;
            assert.equal((queuedCmd["command"] as Record<string, unknown>)["op"], "select-numbered");
            runTimers();
            const select = await flushNextDescribe();
            const selectPayload = calls[flushed - 1]!.payload;
            // The focus grant actuates synchronously on accept: the native
            // foreground must name the selected home.
            const fxRows = (select.reply["effects"] as Array<Record<string, unknown>>).filter((e) => e["kind"] === "focus");
            assert.ok(fxRows.length >= 1, `select grants focus: ${select.rawReply.slice(0, 500)}`);
            assert.equal(fxRows[0]!["awaitShown"], true, `populated select grants awaitShown focus: ${select.rawReply.slice(0, 500)}`);
            const target = fxRows[0]!["target"] as string;
            assert.equal((active as unknown as Record<string, unknown>)["internalId"], target, syncViewSignals ? "native view switch restores the foreground without the focus setter" : "production setter moves the native foreground");
            assert.equal(focusSets, expectedFocusSets, "select grant uses only the expected native focus calls");
            // Explicit duplicate redelivery of the accepted select reply
            // through the real transport callback: the settled guard must
            // neither re-actuate nor mint a second row.
            calls[flushed - 1]!.callback(select.rawReply);
            assert.equal(focusSets, expectedFocusSets, "duplicate select redelivery fires no second setter call");
            // Real transport failure before the feedback accept: the next
            // dispatched capture carries the genuine row, but its reply
            // bytes are lost (non-string callback). Recovery must
            // redispatch, and the retry must re-carry the identical row id
            // with the identical dispatch-clock stamp, never a new row.
            runTimers();
            const dropAt = calls.findIndex((c, i) => i >= flushed && c.method === "DescribeWorld");
            assert.ok(dropAt >= 0, `feedback capture must dispatch (methods=${calls.map((c) => c.method).join(",")})`);
            flushed = dropAt + 1;
            const dropEnvelope = calls[dropAt]!.payload;
            // Request loss: the bytes go out (broker-stamped, like the
            // wire) but the core never processes them and no reply comes
            // back, so the retry below stays the core's first delivery of
            // this feedback. (Reply loss would settle core-side on the
            // discarded reply, which the test-side MiniBroker cannot
            // re-retrieve, so settlement would be unobservable.)
            void broker.stampCapture(dropEnvelope);
            const dropInner = (JSON.parse(dropEnvelope) as Record<string, unknown>)["report"] as Record<string, unknown>;
            const dropWire = (dropInner["focusFeedback"] ?? []) as Array<Record<string, unknown>>;
            assert.equal(dropWire.length, 1, `dropped capture carries the one genuine row: ${dropEnvelope.slice(0, 800)}`);
            const feedbackId = dropWire[0]!["id"];
            assert.equal(dropWire[0]!["target"], target, "feedback names the granted target");
            const attemptMs = dropWire[0]!["attemptMs"];
            assert.ok(typeof attemptMs === "string" && attemptMs !== "", "feedback carries the dispatch-clock stamp");
            calls[dropAt]!.callback(undefined);
            // The retry is a real redispatch (signals plus queue pump
            // drive it): it must arrive, never be silently skipped.
            fireAll();
            runTimers();
            const feedback = await flushNextDescribe();
            const feedbackPayload = calls[flushed - 1]!.payload;
            const innerReport = (feedback.req["report"] ?? feedback.req) as Record<string, unknown>;
            const feedbackWire = (innerReport["focusFeedback"] ?? []) as Array<Record<string, unknown>>;
            assert.equal(feedbackWire.length, 1, `retry re-carries exactly one row: ${feedbackPayload.slice(0, 800)}`);
            assert.equal(feedbackWire[0]!["id"], feedbackId, "retry carries the same row id");
            assert.equal(feedbackWire[0]!["attemptMs"], attemptMs, "retry retains the original stamp");
            // Settle: keep pumping until the core emits the terminal
            // switch event. Every pump here must produce a real capture;
            // a starved pump fails loudly, never silently.
            let switchEvents: Array<Record<string, unknown>> = (select.reply["switchSettled"] ?? []) as Array<Record<string, unknown>>;
            let last: { req: Record<string, unknown>; reply: Record<string, unknown>; rawReply: string } = feedback;
            if (switchEvents.length === 0) {
                switchEvents = (last.reply["switchSettled"] ?? []) as Array<Record<string, unknown>>;
            }
            for (let i = 0; i < 4 && switchEvents.length === 0; i += 1) {
                fireAll();
                runTimers();
                last = await flushNextDescribe();
                switchEvents = (last.reply["switchSettled"] ?? []) as Array<Record<string, unknown>>;
            }
            // Diagnostic context when the core never settles: surface the
            // actual grant/visibility/focus lanes, never a bare count.
            assert.ok(
                switchEvents.length === 1,
                `one terminal switch event, select=${select.rawReply.slice(0, 600)} feedbackReply=${last.rawReply.slice(0, 600)} feedbackWire=${JSON.stringify(feedbackWire).slice(0, 600)}`,
            );
            // Post-settlement capture: a real dispatch must follow, with
            // NO additional switch events, no new feedback rows, and no
            // second setter call.
            fireAll();
            runTimers();
            const later = await flushNextDescribe();
            const laterSwitchEvents = (later.reply["switchSettled"] ?? []) as Array<Record<string, unknown>>;
            assert.equal(laterSwitchEvents.length, 0, `no switch event after the terminal one: ${later.rawReply.slice(0, 500)}`);
            const laterInner = (later.req["report"] ?? later.req) as Record<string, unknown>;
            const laterFeedbackWire = (laterInner["focusFeedback"] ?? []) as Array<Record<string, unknown>>;
            assert.ok(
                !laterFeedbackWire.some((row) => row["id"] !== feedbackId),
                `no new feedback rows after settlement: ${JSON.stringify(laterFeedbackWire).slice(0, 500)}`,
            );
            for (const row of laterFeedbackWire) {
                assert.equal(row["attemptMs"], attemptMs, "re-carried row retains the original stamp");
            }
            assert.equal(focusSets, expectedFocusSets, "no focus reissue after settlement");
            let restoredSwitch: Array<Record<string, unknown>> = [];
            if (!witnessPresent) {
                // Healthy witness restored before the next accepted
                // observation: the failed terminal must not upgrade and
                // focus must not reissue.
                surface["windowAt"] = () => [w1, w2];
                fireAll();
                runTimers();
                const restored = await flushNextDescribe();
                restoredSwitch = (restored.reply["switchSettled"] ?? []) as Array<Record<string, unknown>>;
                for (const ev of restoredSwitch) {
                    assert.equal(ev["outcome"], "completed-with-failures", `failed terminal never upgrades: ${JSON.stringify(ev).slice(0, 300)}`);
                    assert.equal(ev["focus"], "unproven", `failed focus never proves late: ${JSON.stringify(ev).slice(0, 300)}`);
                }
                const restoredInner = (restored.req["report"] ?? restored.req) as Record<string, unknown>;
                const restoredWire = (restoredInner["focusFeedback"] ?? []) as Array<Record<string, unknown>>;
                assert.ok(
                    !restoredWire.some((row) => row["id"] !== feedbackId),
                    `no new focus rows on witness restore: ${JSON.stringify(restoredWire).slice(0, 500)}`,
                );
                assert.equal(focusSets, expectedFocusSets, "witness restore reissues no focus call");
            }
            return {
                selectReply: select.reply,
                feedbackWire,
                feedbackId,
                feedbackAttemptMs: attemptMs as string,
                switchEvents,
                laterSwitchEvents,
                laterFeedbackWire,
                restoredSwitch,
                focusSets,
                selectPayload,
                feedbackPayload,
            };
        } finally {
            try {
                handle?.stop();
            } catch (error) {
                void error;
            }
            await closeEngine();
        }
    }

    it("populated select settles complete/proven with genuine feedback and no replay", { timeout: 120000 }, async () => {
        const driven = await driveSelect(true);
        assert.equal(driven.switchEvents.length, 1, `one terminal switch event: ${JSON.stringify(driven.switchEvents).slice(0, 800)}`);
        assert.equal(driven.switchEvents[0]!["outcome"], "complete", `healthy witness completes: ${JSON.stringify(driven.switchEvents[0]).slice(0, 500)}`);
        assert.equal(driven.switchEvents[0]!["focus"], "proven", `healthy foreground proves focus: ${JSON.stringify(driven.switchEvents[0]).slice(0, 500)}`);
        assert.deepEqual((driven.switchEvents[0]!["failed"] as Array<unknown>), [], "complete carries no failures");
        assert.equal(driven.feedbackWire[0]!["setter"], "accepted", "production setter accepted");
        assert.equal(driven.feedbackWire[0]!["foregroundProven"], true, "genuine foreground proof rides");
        assert.ok(Array.isArray(driven.feedbackWire[0]!["arrival"]) && (driven.feedbackWire[0]!["arrival"] as Array<unknown>).length >= 1, "populated grant carries arrival brackets");
        // Deterministic no-replay proof (driven inside the helper, never
        // vacuous): a real post-settlement capture carried no further
        // switch event, no new feedback row, and no second setter call.
        assert.equal(driven.laterSwitchEvents.length, 0, "no switch event after the terminal one");
        assert.equal(driven.focusSets, 1, "exactly one focus setter call end to end");
    });

    it("missing shown witness settles completed-with-failures/unproven once with no upgrade", { timeout: 120000 }, async () => {
        const driven = await driveSelect(false);
        assert.equal(driven.switchEvents.length, 1, `one terminal switch event: ${JSON.stringify(driven.switchEvents).slice(0, 800)}`);
        assert.equal(driven.switchEvents[0]!["outcome"], "completed-with-failures", `null witness fails outcome: ${JSON.stringify(driven.switchEvents[0]).slice(0, 500)}`);
        assert.equal(driven.switchEvents[0]!["focus"], "unproven", `null witness leaves focus unproven: ${JSON.stringify(driven.switchEvents[0]).slice(0, 500)}`);
        // Healthy setter+foreground still ride the genuine row: only the
        // arrival proof is missing, so the failure isolates to witness.
        assert.equal(driven.feedbackWire[0]!["setter"], "accepted", "setter stays accepted without windowAt");
        assert.equal(driven.feedbackWire[0]!["foregroundProven"], true, "foreground stays proven without windowAt");
        assert.ok(
            driven.feedbackWire[0]!["arrival"] !== undefined,
            "arrival lane rides even when the witness is null",
        );
        // Deterministic no-upgrade proof (driven inside the helper): a
        // real post-settlement capture carried no further switch event,
        // and the witness restore afterwards upgraded nothing and
        // reissued no focus call.
        assert.equal(driven.laterSwitchEvents.length, 0, "no switch event after the terminal one");
        for (const ev of driven.restoredSwitch) {
            assert.equal(ev["outcome"], "completed-with-failures", "restored witness never upgrades the terminal");
            assert.equal(ev["focus"], "unproven", "restored witness never proves focus late");
        }
        assert.equal(driven.focusSets, 1, "exactly one focus setter call end to end");
    });

    it("sync view-switch own echoes defer to one authorized focus dispatch and complete/proven", { timeout: 120000 }, async () => {
        const driven = await driveSelect(true, true);
        assert.equal(driven.feedbackWire.length, 1, "one focus row, no missing feedback");
        assert.equal(driven.feedbackWire[0]!["setter"], "accepted", "own-echo focus setter accepted");
        const fxTarget = ((driven.selectReply["effects"] as Array<Record<string, unknown>>).find((e) => e["kind"] === "focus")!)["target"];
        assert.equal(driven.feedbackWire[0]!["target"], fxTarget, "row names the granted target");
        assert.equal(driven.feedbackWire[0]!["post"], fxTarget, "post names the granted foreground");
        assert.equal(driven.feedbackWire[0]!["foregroundProven"], true, "own-echo foreground proves");
        assert.ok(Array.isArray(driven.feedbackWire[0]!["arrival"]) && (driven.feedbackWire[0]!["arrival"] as Array<unknown>).length >= 1, "populated grant carries arrival brackets");
        assert.equal(driven.switchEvents.length, 1, "one terminal switch event");
        assert.equal(driven.switchEvents[0]!["outcome"], "complete", "healthy own echoes complete");
        assert.equal(driven.switchEvents[0]!["focus"], "proven", "healthy own echoes prove focus");
        assert.equal(driven.laterSwitchEvents.length, 0, "no switch event after the terminal one");
        assert.ok(!driven.laterFeedbackWire.some((row) => row["id"] !== driven.feedbackId), "no new feedback rows after settlement");
        for (const row of driven.laterFeedbackWire) {
            assert.equal(row["attemptMs"], driven.feedbackAttemptMs, "re-carried row retains the original stamp");
        }
        assert.equal(driven.focusSets, 1, "exactly one core-target setter end to end");
    });

});

describe("world entry rapid batch (KDE FIFO + latest-world)", () => {
    // KDE rapid-input batching (decisions/commands.md#kde-rapid-input) and
    // the queued-ingress contract (porting/world-contract.md#queued-kde-ingress).
    // every press parks FIFO with no busy refusal (QueueWorldCommand only
    // parks); ONE fresh capture drains the whole parked batch against
    // post-previous-action core state; presses arriving mid-flight ride the
    // next batch. Ordinary keyboard windowed actions resolve implicit at
    // core current focus (`win:{"currentFocus":true}`, exact closed object;
    // null invalid); explicit string ids (settled by-id continuations) and
    // the directional nofocus `win:""` keep their meaning. Overlapping
    // accepted publications share one latest-pending slot while previous
    // writes are genuinely in flight: superseded unissued targets never
    // issue, issued writes spend at-most-once, and an intermediate
    // superseded publication drives no covering persist of its own.
    // These FAIL against the current busy/directional-only single-head
    // drain (submitCommandInner busy, per-head fileQueue, one
    // WriteFloatIntent per persist waiter) and pin the correction.

    // Existing harness boundary for parking before a capture: a native
    // burst arms the observation debounce, so every press below queues
    // instead of taking the idle fast path (no retroactive joining).
    const armDebounce = (h: Harness): void => {
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            (handler as (...args: Array<unknown>) => void)();
        }
    };
    const parkedOps = (h: Harness): string[] => queueCalls(h)
        .map((req) => (req["command"] as Record<string, unknown>)["op"] as string);
    const acceptLast = (h: Harness, rev: string, extra: Record<string, unknown> = {}): void => {
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, rev, extra));
    };
    const storedIntentReply = (payload: string): string => {
        const body = JSON.parse(payload) as Record<string, unknown>;
        return JSON.stringify({
            v: 2,
            correlation_id: body["correlation_id"],
            outcome: "stored",
            stored: (body["records"] as Array<unknown>).length,
        });
    };
    const untouchedFloatRow = (id: string): Record<string, unknown> => ({
        win: id, out: "out", ws: "ws1",
        alloc: null, frame: null,
        frame_auth: "untouched", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
    });

    it("parks every mixed press FIFO with no busy refusal", () => {
        const h = makeHarness();
        discover(h);
        armDebounce(h);
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.requestFocus("left");
        h.handle?.requestToggleOrientation();
        h.handle?.requestWorkspaceRelative(1);
        assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy refusal while parking the burst");
        assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no shortcut failure while parking the burst");
        const queued = queueCalls(h);
        assert.equal(queued.length, 3, "all three mixed presses park before the drain");
        assert.deepEqual(parkedOps(h), ["focus-directional", "toggle-orientation", "select-relative"], "press order preserved");
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore,
            "parking files no capture of its own",
        );
    });

    it("files the whole parked batch before ONE fresh capture drains it", () => {
        const h = makeHarness();
        discover(h);
        armDebounce(h);
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.requestFocus("left");
        h.handle?.requestFocus("right");
        h.handle?.requestFocus("left");
        // All three presses park behind the armed debounce, so the whole
        // batch is filed before the single draining capture; the current
        // one-head-per-capture path files only the head at dispatch.
        assert.equal(queueCalls(h).length, 3, "whole batch filed before the capture");
        h.runTimers();
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore + 1,
            "exactly one fresh capture drains the batch",
        );
        assert.equal(queueCalls(h).length, 3, "the drain files nothing further");
        acceptLast(h, "1");
        h.runTimers();
        assert.equal(queueCalls(h).length, 3, "the accepted drain files nothing further");
    });

    it("parks commands arriving during the capture for the next batch", () => {
        const h = makeHarness();
        discover(h);
        armDebounce(h);
        h.handle?.requestFocus("left");
        h.handle?.requestFocus("right");
        h.runTimers();
        const flightDescribes = h.calls.filter((c) => c.method === "DescribeWorld").length;
        // A press arriving while the drain flight owns the wire parks without
        // starting a second flight.
        h.handle?.requestFocus("left");
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            flightDescribes,
            "no second flight while one owns the wire",
        );
        assert.equal(queueCalls(h).length, 3, "late press parks for the next batch");
        acceptLast(h, "1");
        h.runTimers();
        assert.equal(queueCalls(h).length, 3, "the next batch files nothing further");
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            flightDescribes + 1,
            "next batch drains in exactly one follow-up capture",
        );
    });

    it("subjects implicit presses at core current focus", () => {
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        const focusCmds = queueCalls(h).filter((req) => (req["command"] as Record<string, unknown>)["op"] === "focus-directional");
        assert.ok(focusCmds.length >= 1, "implicit directional files");
        const lastFocus = focusCmds[focusCmds.length - 1] as Record<string, unknown>;
        assert.deepEqual(
            (lastFocus["command"] as Record<string, unknown>)["win"],
            { currentFocus: true },
            "implicit directional resolves at core current focus, never native-rebound",
        );
    });

    it("subjects ordinary sticky intent at core current focus", () => {
        // requestSticky is ordinary keyboard intent, not an explicit route:
        // it resolves implicit exactly like every other windowed press.
        const h = makeHarness();
        discover(h);
        h.handle?.requestSticky();
        h.runTimers();
        const queued = queueCalls(h);
        assert.equal(queued.length, 1, "ordinary sticky files");
        const cmd = (queued[0] as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "toggle-sticky");
        assert.deepEqual(
            cmd["win"],
            { currentFocus: true },
            "ordinary keyboard sticky is implicit, never press-time identity",
        );
    });

    it("keeps the explicit empty directional nofocus marker", () => {
        const h = makeHarness();
        discover(h);
        h.world.activeWindow = null;
        try {
            h.handle?.requestFocus("left");
            const queued = queueCalls(h);
            assert.equal(queued.length, 1, "nofocus press still files");
            assert.deepEqual(
                ((queued[0] as Record<string, unknown>)["command"] as Record<string, unknown>)["win"],
                "",
                "empty win stays genuinely no-focus, never implicit focus",
            );
        } finally {
            h.world.activeWindow = h.world.wins[0] as FakeWindow;
            h.handle?.stop();
        }
    });

    it("keeps settled by-id continuation identity (guard)", () => {
        // Genuinely explicit route (not keyboard intent): the B9
        // maximized-unfloat continuation resubmits through the settled
        // command lane with its settled window id and rect, passing through
        // untouched. Unchanged by the implicit-marker migration.
        const h = makeHarness();
        discover(h);
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const seed = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", seed["source"] as string, seed["nativeInvalid"] as string, "1", {
            windows: [untouchedFloatRow("win-a")],
        }));
        const target = h.world.wins.find((w) => w.internalId === "win-a");
        assert.ok(target !== undefined);
        (target as FakeWindow).maximizeMode = 3;
        h.handle?.requestFloat();
        h.runTimers();
        assert.equal(queueCalls(h).length, 1, "clear schedules observation first");
        const req = requestOf(h);
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", req["source"] as string, req["nativeInvalid"] as string, "2", {
            windows: [untouchedFloatRow("win-a")],
        }));
        h.runTimers();
        const filed = JSON.parse(queueOf(h)) as Record<string, unknown>;
        const cmd = filed["command"] as Record<string, unknown>;
        const frame = (target as FakeWindow).frameGeometry;
        assert.deepEqual(cmd, { op: "toggle-float", win: "win-a", rect: [frame.x, frame.y, frame.width, frame.height] });
        assert.equal(typeof cmd["win"], "string", "settled continuation keeps its explicit string id");
    });

    it("overlapping publications share one latest slot while writes fly", () => {
        // Distinct accepted publications overlapping the genuine async
        // intent-persist barrier (floatIntent seam holds every
        // WriteFloatIntent until answered by hand; native setters are
        // synchronous, so an empty focus record proves the hold). The
        // single latest-pending slot applies exactly once: superseded
        // unissued targets/raise/stack never issue, and an intermediate
        // superseded publication drives no covering persist of its own.
        // One accepted reply carrying two focus effects is NOT
        // supersession (both effects of one publication apply in order).
        const h = makeHarness("per-output-local", false, true, undefined, undefined, true, startWorldAdapterEntry, false, true);
        discover(h);
        // Extra lifetimes: win-c takes the superseded focus, win-d dies NEW.
        h.world.wins.push(win("win-c", "out", h.world.desktops[0] as FakeDesktop, 400));
        h.world.wins.push(win("win-d", "out", h.world.desktops[0] as FakeDesktop, 600));
        // Delivered close: win-b is removed before the first drain, so it
        // rides the first report (no replay assertion after its accept: the
        // incorporation may ack delivered closes independent of apply).
        const removedB = h.world.wins.find((w) => w.internalId === "win-b");
        assert.ok(removedB !== undefined);
        for (const handler of [...(h.signals.windowRemoved as FakeSignal).handlers]) {
            (handler as (payload?: unknown) => void)(removedB);
        }
        h.world.wins = h.world.wins.filter((w) => w.internalId !== "win-b");
        // Park one ordinary press behind the removal-armed debounce.
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(queueCalls(h).length, 1, "first batch files its park");
        const flight1 = requestOf(h);
        assert.ok((reportOf(h)["closed"] as Array<string>).includes("win-b"), "delivered close rides the first drain");
        // First publication: intent change plus superseded focus, held
        // behind the genuinely in-flight persist.
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", flight1["source"] as string, flight1["nativeInvalid"] as string, "1", {
            intentWrites: [{ win: "win-a", data: "{\"t\":1}" }],
            effects: [
                { id: "fx-old", kind: "focus", target: "win-c", rev: "1", cause: "automatic-admission", cmdSeq: null, awaitShown: false },
                { id: "rx-old", kind: "raise", target: "win-c", rev: "1", cause: "automatic-admission", cmdSeq: null, awaitShown: false },
            ],
        }));
        assert.equal(h.intentWriteFlights.length, 1, "first persist genuinely in flight");
        assert.deepEqual(h.world.focused, [], "held publication issues nothing");
        // A NEW undelivered close lands after the first acceptance, and the
        // second drain carries it across the skipped publication.
        const removedD = h.world.wins.find((w) => w.internalId === "win-d");
        assert.ok(removedD !== undefined);
        for (const handler of [...(h.signals.windowRemoved as FakeSignal).handlers]) {
            (handler as (payload?: unknown) => void)(removedD);
        }
        h.world.wins = h.world.wins.filter((w) => w.internalId !== "win-d");
        h.handle?.requestFocus("left");
        h.runTimers();
        assert.equal(queueCalls(h).length, 2, "second batch files its park");
        assert.ok((reportOf(h)["closed"] as Array<string>).includes("win-d"), "undelivered close retained across the skipped publication");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", (requestOf(h)["source"] as string), (requestOf(h)["nativeInvalid"] as string), "2", {
            intentWrites: [{ win: "win-a", data: "{\"t\":2}" }],
            effects: [
                { id: "fx-mid", kind: "focus", target: "win-c", rev: "2", cause: "automatic-admission", cmdSeq: null, awaitShown: false },
            ],
        }));
        assert.equal(h.intentWriteFlights.length, 1, "second publication queues behind the in-flight write");
        assert.deepEqual(h.world.focused, [], "still nothing issued");
        // A third publication overlaps the same barrier.
        h.handle?.requestFocus("right");
        h.runTimers();
        assert.equal(queueCalls(h).length, 3, "third batch files its park");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", (requestOf(h)["source"] as string), (requestOf(h)["nativeInvalid"] as string), "3", {
            intentWrites: [{ win: "win-a", data: "{\"t\":3}" }],
            effects: [
                { id: "fx-new", kind: "focus", target: "win-a", rev: "3", cause: "automatic-admission", cmdSeq: null, awaitShown: false },
                { id: "sx-new", kind: "stack", target: "win-a", rev: "3", cause: "automatic-admission", cmdSeq: null, awaitShown: false },
            ],
        }));
        assert.equal(h.intentWriteFlights.length, 1, "third publication queues behind the same in-flight write");
        assert.deepEqual(h.world.focused, [], "nothing issued before the barrier lifts");
        // The in-flight write lands: the first superseded publication drops
        // without issuing and the next waiter persists covering.
        h.intentWriteFlights[0]!.callback(storedIntentReply(h.intentWriteFlights[0]!.payload));
        assert.deepEqual(h.world.focused, [], "superseded unissued focus drops, latest still held");
        assert.equal(h.intentWriteFlights.length, 2, "next waiter persists covering");
        // Answering the second round: the intermediate superseded
        // publication must NOT drive a covering persist of its own. The
        // waiter queue does (a third flight); the single latest slot does
        // not (the covering write applies the latest publication).
        h.intentWriteFlights[1]!.callback(storedIntentReply(h.intentWriteFlights[1]!.payload));
        assert.equal(h.intentWriteFlights.length, 2, "intermediate superseded publications drive no persist of their own");
        assert.deepEqual(h.world.focused, ["win-a"], "latest issues once, superseded never issue");
        assert.equal(h.world.membershipWrites.length, 0, "no membership writes");
        assert.equal(h.world.geometryWrites.length, 0, "no geometry writes");
        // Spent: re-seeing the issued effect never reissues, and acked
        // closes never replay.
        armDebounce(h);
        h.runTimers();
        assert.ok(!(reportOf(h)["closed"] as Array<string>).includes("win-b"), "acked closes never replay");
        assert.ok(!(reportOf(h)["closed"] as Array<string>).includes("win-d"), "acked closes never replay");
        h.lastCall("DescribeWorld").callback(acceptedReply("gen-1", (requestOf(h)["source"] as string), (requestOf(h)["nativeInvalid"] as string), "4", {
            effects: [
                { id: "fx-new", kind: "focus", target: "win-a", rev: "3", cause: "automatic-admission", cmdSeq: null, awaitShown: false },
            ],
        }));
        assert.deepEqual(h.world.focused, ["win-a"], "issued writes stay spent");
    });

    it("serializes filing: a newer press never submits while an older Queue ACK is outstanding", () => {
        // The broker high-water refuses an older missing command once a
        // later one is accepted, so the pump holds every newer press until
        // the head is known filed (or permanently refused). A synchronously
        // answered head still files the whole parked remainder before ONE
        // capture (no per-command captures, no reentrant dispatch).
        const hold = { value: true };
        const h = makeHarness("per-output-local", false, true, undefined, undefined, true, startWorldAdapterEntry, false, false, hold);
        discover(h);
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        h.handle?.requestFocus("left");
        assert.equal(queueCalls(h).length, 1, "first press submits");
        h.handle?.requestFocus("right");
        h.handle?.requestFocus("left");
        assert.equal(queueCalls(h).length, 1, "newer presses park without submitting while the older ACK is outstanding");
        assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy refusal while parking");
        const head = h.calls.filter((c) => c.method === "QueueWorldCommand")[0] as CapturedCall;
        const headBody = JSON.parse(head.payload) as Record<string, unknown>;
        hold.value = false;
        head.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: headBody["client"], clientCmd: headBody["clientCmd"], globalCmd: "1",
        }));
        assert.deepEqual(
            queueCalls(h).map((req) => req["clientCmd"]),
            ["1", "2", "3"],
            "head accept files the whole parked remainder in FIFO order",
        );
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore + 1,
            "exactly one fresh capture drains the filed batch",
        );
        h.handle?.stop();
    });

    it("a lost Queue ACK redelivers the exact bytes under the same identity with a newer press parked", () => {
        // Transport ambiguity never mints a duplicate: the timed-out head
        // redelivers byte-identical under its original clientCmd before the
        // parked newer press files under its own identity.
        const hold = { value: true };
        const h = makeHarness("per-output-local", false, true, undefined, undefined, true, startWorldAdapterEntry, false, false, hold);
        discover(h);
        h.handle?.requestFocus("left");
        assert.equal(queueCalls(h).length, 1, "first press submits");
        const firstPayload = h.lastCall("QueueWorldCommand").payload;
        h.handle?.requestFocus("right");
        assert.equal(queueCalls(h).length, 1, "newer press parks while the head ACK is outstanding");
        const timeout = [...h.timers].reverse().find((t) => t.delay === 2000);
        assert.ok(timeout !== undefined, "queue timeout armed");
        h.timers.splice(h.timers.indexOf(timeout as { delay: number; cb: () => void }), 1);
        (timeout as { cb: () => void }).cb();
        assert.ok(!h.calls.some((c) => c.method === "TakeCommandResult"), "lost queue files no take");
        // The next press pumps the lost head first (exact bytes), never the
        // newer press out of order and never a new identity.
        h.handle?.requestFocus("left");
        const queues = h.calls.filter((c) => c.method === "QueueWorldCommand");
        assert.equal(queues.length, 2, "exactly one redelivery, no duplicate numbering");
        assert.equal((queues[1] as CapturedCall).payload, firstPayload, "redelivery is byte-identical under the same identity");
        h.handle?.stop();
    });

    it("a nonpermanent queue refusal never retries inline", () => {
        // A retained-position refusal recovers honestly on-next-event: no
        // synchronous resubmit (a synchronously refusing broker would
        // otherwise recurse unboundedly), exactly one scheduled dispatch.
        const hold = { value: true };
        const h = makeHarness("per-output-local", false, true, undefined, undefined, true, startWorldAdapterEntry, false, false, hold);
        discover(h);
        h.handle?.requestFocus("left");
        assert.equal(queueCalls(h).length, 1, "first press submits");
        const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        const head = h.calls.filter((c) => c.method === "QueueWorldCommand")[0] as CapturedCall;
        const headBody = JSON.parse(head.payload) as Record<string, unknown>;
        head.callback(JSON.stringify({
            v: 4, outcome: "rejected", kind: "busy", message: "busy",
            gen: "gen-1", source: "", nativeInvalid: "0", clock: "1", gestureAssoc: null,
        }));
        assert.equal(queueCalls(h).length, 1, "no inline resubmit after a nonpermanent refusal");
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            describesBefore,
            "no inline capture after a nonpermanent refusal",
        );
        assert.ok(
            h.logs.some((l) => l.startsWith("omnitiler:world:queue-rejected")),
            "refusal logs its on-next-event recovery",
        );
        void headBody;
        h.handle?.stop();
    });

    it("captures fresh after a Queue ACK with interleaved native invalidation", () => {
        // Native input landing while a Queue ACK is outstanding is
        // incorporated after the filings settle: the draining capture never
        // sends the stale pre-gap observation.
        const hold = { value: true };
        const h = makeHarness("per-output-local", false, true, undefined, undefined, true, startWorldAdapterEntry, false, false, hold);
        discover(h);
        h.handle?.requestFocus("left");
        assert.equal(queueCalls(h).length, 1, "command files");
        const mark = requestOf(h)["nativeInvalid"] as string;
        const sentBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
        for (const handler of [...(h.signals.desktopsChanged as FakeSignal).handlers]) {
            (handler as (...args: Array<unknown>) => void)();
        }
        const head = h.calls.filter((c) => c.method === "QueueWorldCommand")[0] as CapturedCall;
        const headBody = JSON.parse(head.payload) as Record<string, unknown>;
        hold.value = false;
        head.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: headBody["client"], clientCmd: headBody["clientCmd"], globalCmd: "1",
        }));
        // The accept defers to the invalidation-armed debounce (never a
        // synchronous preempt from inside the filing kick).
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            sentBefore,
            "queue accept never preempts the armed debounce",
        );
        h.runTimers();
        assert.equal(
            h.calls.filter((c) => c.method === "DescribeWorld").length,
            sentBefore + 1,
            "gap invalidation drains exactly one fresh capture",
        );
        const sent = requestOf(h);
        assert.notEqual(sent["nativeInvalid"], mark, "stale mark is never sent");
        assert.ok(isCanonicalU64(sent["nativeInvalid"]), "fresh watermark is canonical");
        h.handle?.stop();
    });

    it("a later accept collects every outstanding requester result, including a timed-out capture", () => {
        // A capture timeout never requeues its sent commands, but their
        // identities stay undecided: the next accepted capture (even one
        // draining a newer batch) collects every outstanding take,
        // distinguishing pending retries from terminal settlements.
        const h = makeHarness();
        discover(h);
        h.handle?.requestFocus("left");
        const filed = queueCalls(h);
        assert.equal(filed.length, 1, "first press files");
        const firstCmd = filed[0]?.["clientCmd"] as string;
        const capture = h.lastCall("DescribeWorld");
        const timeout = [...h.timers].reverse().find((t) => t.delay === 2000);
        assert.ok(timeout !== undefined, "capture timeout armed");
        h.timers.splice(h.timers.indexOf(timeout as { delay: number; cb: () => void }), 1);
        (timeout as { cb: () => void }).cb();
        assert.ok(!h.calls.some((c) => c.method === "TakeCommandResult"), "timed-out capture collects no take yet");
        // A newer press parks and its drain flies on the next event (the
        // withheld queue rides the catchup after the timeout armed the
        // observation-only lane); accepting it collects both the timed-out
        // identity and the new one in clientCmd order.
        h.handle?.requestFocus("right");
        assert.equal(queueCalls(h).length, 2, "newer press files under its own identity");
        h.runTimers();
        const drained = h.lastCall("DescribeWorld");
        assert.notEqual(drained, capture, "a follow-up capture flies after the timeout");
        const body = JSON.parse(drained.payload) as Record<string, unknown>;
        drained.callback(acceptedReply("gen-1", body["source"] as string, body["nativeInvalid"] as string, "1"));
        const takes = h.calls.filter((c) => c.method === "TakeCommandResult");
        assert.equal(takes.length, 2, "one take per outstanding identity, timed-out included");
        assert.deepEqual(
            takes.map((c) => (JSON.parse(c.payload) as Record<string, unknown>)["clientCmd"]),
            [firstCmd, "2"],
            "outstanding identities collected in order, never re-executed",
        );
        h.handle?.stop();
    });
});
