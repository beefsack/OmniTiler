// Scope authorization at the ordinary-press ingress: with a non-empty
// scope class, an ordinary press whose live subject is foreign must
// never file a QueueWorldCommand. Every current press seam derives its
// subject from the foreign-filtered active-window gate, so this locks
// the end-to-end refusal; the central submitCommand original-target
// check behind it is defense-in-depth for any future explicit-target
// caller (the implicit-focus marker must never launder a foreign id).
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop {
    id: string;
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
    resourceClass: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: null;
    maxSize: null;
}
interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

const desk = (id: string): FakeDesktop => ({ id });

function tiledWin(id: string, cls: string, out: string, ws: FakeDesktop, x: number): FakeWindow {
    return {
        internalId: id, resourceClass: cls, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
    };
}

interface Harness {
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    shortcuts: Map<string, () => void>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: () => void;
    setActive: (win: FakeWindow | null) => void;
}

function makeScopedHarness(foreign: FakeWindow, domestic: FakeWindow): Harness {
    const ws = foreign.desktops[0] as FakeDesktop;
    const world = {
        screens: [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }],
        desktops: [ws],
        wins: [foreign, domestic],
        activeScreen: { name: "out-a" },
        activeWindow: foreign as FakeWindow | null,
        currentByScreen: new Map([["out-a", ws]]),
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const shortcuts = new Map<string, () => void>();
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        get activeWindow() {
            return world.activeWindow;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
            const found = world.desktops.find((d) => (d as unknown) === deskRef);
            if (found) {
                world.currentByScreen.set(screen.name, found);
            }
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null,
        removeDesktop: () => false,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        scopeClass: "OmniTilerTestWindow",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
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
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: (name, _text, _seq, cb) => {
            shortcuts.set(name, cb);
            return true;
        },
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    const runTimers = (): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay > 120) {
                continue;
            }
            timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    };
    return {
        calls, timers, shortcuts, handle, runTimers,
        setActive: (win) => {
            world.activeWindow = win;
        },
    };
}

function answerHandshake(h: Harness, gen = "gen-1"): void {
    const disc = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(disc !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse((disc as CapturedCall).payload) as Record<string, unknown>)["req"] as string;
    (disc as CapturedCall).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: Harness, gen = "gen-1"): void {
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

function acceptedAutoReply(gen: string, source: string, nativeInvalid: string, rev: string): string {
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "0",
        gestureSettled: null, gestureAssoc: null,
        closeFocus: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}

function discover(h: Harness, gen = "gen-1"): void {
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
    const auto = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(auto !== undefined, "expected DescribeWorld after settings accept");
    const autoBody = JSON.parse((auto as CapturedCall).payload) as Record<string, unknown>;
    (auto as CapturedCall).callback(acceptedAutoReply(gen, autoBody["source"] as string, autoBody["nativeInvalid"] as string, "1"));
}

function queueCommands(h: Harness): Array<Record<string, unknown>> {
    return h.calls
        .filter((c) => c.method === "QueueWorldCommand")
        .map((c) => (JSON.parse(c.payload) as { command: Record<string, unknown> }).command);
}

describe("world scope command ingress", () => {
    it("files nothing for ordinary presses under a foreign focus", () => {
        const ws = desk("ws-a");
        const foreign = tiledWin("win-foreign", "firefox", "out-a", ws, 0);
        const domestic = tiledWin("win-domestic", "OmniTilerTestWindow", "out-a", ws, 210);
        const h = makeScopedHarness(foreign, domestic);
        discover(h);
        h.handle?.requestSticky();
        h.handle?.requestMaximize();
        h.handle?.requestFullscreen();
        h.handle?.requestMove("right");
        h.handle?.requestFocus("right");
        h.handle?.requestResize("right", "outwards");
        h.handle?.requestToggleOrientation();
        h.shortcuts.get("omnitiler-toggle-sticky")?.();
        h.shortcuts.get("omnitiler-toggle-maximize")?.();
        h.shortcuts.get("omnitiler-focus-right")?.();
        h.runTimers();
        assert.deepEqual(queueCommands(h), [], "no foreign press target rides the wire");
        h.handle?.stop();
    });

    it("still files the implicit marker for an in-scope press", () => {
        const ws = desk("ws-a");
        const foreign = tiledWin("win-foreign", "firefox", "out-a", ws, 0);
        const domestic = tiledWin("win-domestic", "OmniTilerTestWindow", "out-a", ws, 210);
        const h = makeScopedHarness(foreign, domestic);
        discover(h);
        h.setActive(domestic);
        h.handle?.requestSticky();
        h.runTimers();
        const filed = queueCommands(h);
        assert.equal(filed.length, 1, "one in-scope press files");
        assert.deepEqual(filed[0], { op: "toggle-sticky", win: { currentFocus: true } });
        h.handle?.stop();
    });
});
