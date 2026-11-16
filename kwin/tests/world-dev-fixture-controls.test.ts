// Development-only fixture-scoped deterministic omission & preview-reply
// skip controls: exact OmniTilerTestWindow scope, trace-only compilation,
// one-shot auto-disarm, generation/stop cleanup, and preserved
// preview-only + terminal/result paths.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";

import { startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface CapturedCall {
    service: string;
    path: string;
    iface: string;
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

function gestureSnapshotJson(over: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 1,
        instance: "gesture-1",
        serial: "1",
        revision: "1",
        phase: "updated",
        win: "win-a",
        kind: "move",
        startRect: [0, 0, 200, 200],
        startCursor: [10, 10],
        cursor: [25, 25],
        modalMove: 1,
        changed: 1,
        superseded: 0,
        available: 1,
        ...over,
    });
}

function makeHarness(scopeClass?: string | readonly string[], opts: { floatIntent?: boolean; onLog?: (line: string) => void } = {}) {
    const ws1 = { id: "ws1" };
    const geometryWrites: string[] = [];
    const winA = {
        internalId: "win-a",
        resourceClass: "OmniTilerTestWindow",
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        dialog: false,
        modal: false,
        wantsInput: true,
        normalWindow: true,
        output: { name: "out" },
        desktops: [ws1],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
        _geom: { x: 0, y: 0, width: 200, height: 200 },
        get frameGeometry() {
            return { ...this._geom };
        },
        set frameGeometry(rect: { x: number; y: number; width: number; height: number }) {
            geometryWrites.push(`${rect.x},${rect.y},${rect.width},${rect.height}`);
            this._geom = { ...rect };
        },
    };
    const windowActivatedHandlers: Array<(w: unknown) => void> = [];
    const surface = {
        windowList: () => [winA],
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1],
        activeScreen: { name: "out" },
        activeWindow: winA,
        currentDesktopForScreen: () => ws1,
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
        windowAdded: { connect: () => {}, disconnect: () => {} },
        windowRemoved: { connect: () => {}, disconnect: () => {} },
        windowActivated: {
            connect: (fn: (w: unknown) => void) => windowActivatedHandlers.push(fn),
            disconnect: () => {},
        },
        currentDesktopChanged: { connect: () => {}, disconnect: () => {} },
        screensChanged: { connect: () => {}, disconnect: () => {} },
        virtualDesktopsChanged: { connect: () => {}, disconnect: () => {} },
    };

    const calls: CapturedCall[] = [];
    let latestWaitWorld: CapturedCall | null = null;
    const gestureCalls: CapturedCall[] = [];
    const presentationCalls: CapturedCall[] = [];
    const logs: string[] = [];
    const shortcuts = new Map<string, { text: string; sequence: string; cb: () => void }>();
    const timers: Array<{ delay: number; cb: () => void; cancelled: boolean }> = [];
    let currentGen = "gen-1";
    let rev = 0;
    let obs = 0;

    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "test-owner",
        adapter: "test-adapter",
        ...(scopeClass !== undefined ? { scopeClass } : {}),
        ...(opts.floatIntent === true ? { floatIntent: true } : {}),
        callDbus: (service, path, iface, method, payload, callback) => {
            if (method === "ReadFloatIntent") {
                const body = JSON.parse(payload) as { correlation_id?: string };
                callback(JSON.stringify({
                    v: 2,
                    correlation_id: body.correlation_id ?? "1",
                    outcome: "ok",
                    records: [],
                    stored: 0,
                    returned: 0,
                }));
                return;
            }
            if (method === "ReadWorldPresentation") {
                presentationCalls.push({ service, path, iface, method, payload, callback });
                const body = JSON.parse(payload) as { req?: string; nativeInvalid?: string };
                callback(JSON.stringify({
                    v: 4,
                    gen: currentGen,
                    req: body.req ?? "1",
                    rev: String(rev),
                    obs: String(obs),
                    nativeInvalid: body.nativeInvalid ?? "0",
                    presentation: DISABLED_PRESENTATION,
                    preview: null,
                }));
                return;
            }
            if (method === "WaitWorldWake") {
                latestWaitWorld = { service, path, iface, method, payload, callback };
            }
            calls.push({ service, path, iface, method, payload, callback });
        },
        gestureCallDbus: (service, path, iface, method, payload, callback) => {
            gestureCalls.push({ service, path, iface, method, payload, callback });
        },
        effectCallDbus: () => {},
        scheduleOnce: (delay, cb) => {
            const entry = { delay, cb, cancelled: false };
            timers.push(entry);
            return () => {
                entry.cancelled = true;
            };
        },
        log: (m) => {
            opts.onLog?.(m);
            logs.push(m);
        },
        registerShortcutFn: (name, text, sequence, cb) => {
            shortcuts.set(name, { text, sequence, cb });
            return true;
        },
        readWorkspaceModeFn: () => "per-output-local",
        readProfileFn: () => "cosmic",
        readDefaultTiledFn: () => true,
        readInnerGapFn: () => 8,
        readOuterGapFn: () => 8,
        readSameAxisMoveFn: () => "group-with-neighbor",
        readBornMaximizedFn: () => false,
        readMigrationSourceRefillFn: () => false,
    });

    const settleHandshake = (gen = currentGen): void => {
        currentGen = gen;
        let progressed = true;
        while (progressed) {
            progressed = false;
            const remaining: CapturedCall[] = [];
            for (const call of calls.splice(0)) {
                if (call.method === "DiscoverWorld") {
                    const body = JSON.parse(call.payload) as { type?: string; req?: string; rev?: string };
                    if (body.type === "handshake") {
                        progressed = true;
                        call.callback(JSON.stringify({
                            type: "handshake-reply",
                            v: 4,
                            adapter: "test-adapter",
                            req: body.req ?? "1",
                            ready: false,
                            kind: "",
                            gen,
                        }));
                    } else if (body.type === "settings") {
                        progressed = true;
                        call.callback(JSON.stringify({
                            type: "settings-reply",
                            v: 4,
                            adapter: "test-adapter",
                            gen,
                            req: body.req ?? "1",
                            rev: body.rev ?? "1",
                            status: "accepted",
                            diagnostics: [],
                        }));
                    } else {
                        remaining.push(call);
                    }
                } else {
                    remaining.push(call);
                }
            }
            calls.push(...remaining);
        }
    };

    const nextReply = (
        payload: string,
        opts: {
            dragPreview?: Record<string, unknown> | null;
            winRect?: [number, number, number, number];
            gestureSettled?: { id: string; committed: boolean } | null;
            gestureAssoc?: { instance: string; serial: string; coreId: string } | null;
            gestureSettledAssoc?: { instance: string; serial: string; coreId: string } | null;
            intentDesired?: Array<{ win: string; data: string | null }>;
            wsMode?: "tiled" | "floating";
            windows?: Array<Record<string, unknown>>;
            transfers?: Array<Record<string, unknown>>;
            nativeCreations?: Array<Record<string, unknown>>;
            focusRequest?: Record<string, unknown> | null;
            operationSettled?: Array<Record<string, unknown>>;
        } = {},
    ): string => {
        rev += 1;
        obs += 1;
        const body = JSON.parse(payload) as { source: string; nativeInvalid: string; gen: string };
        const rect = opts.winRect ?? [0, 0, 200, 200];
        const defaultWindows = [{
            win: "win-a",
            out: "out",
            ws: "wl-1",
            alloc: rect,
            frame: rect,
            frame_auth: "controlled",
            frame_hold: "",
            member_auth: "controlled",
            member_hold: "",
            frame_op: "",
            clear_op: "",
            floating: false,
            float_origin: "",
            cmdSeq: null,
            visibility: null,
            visibility_auth: "untouched",
            visibility_hold: "",
            state: [],
        }];
        const text = JSON.stringify({
            v: 4,
            adapter: "test-adapter",
            gen: body.gen,
            req: body.source,
            obs: String(obs),
            rev: String(rev),
            ready: true,
            consumed: false,
            scopes: [],
            workspaces: [{ id: "wl-1", out: "out", bind: "ws1", scope: { kind: "output", out: "out" }, order: 0, mode: opts.wsMode ?? "tiled", exist: true, hold: "" }],
            outputs: [],
            windows: opts.windows ?? defaultWindows,
            transfers: opts.transfers ?? [],
            effects: [],
            focusRequest: opts.focusRequest ?? null,
            dragPreview: opts.dragPreview ?? null,
            gestureSettled: opts.gestureSettled ?? null,
            gestureAssoc: opts.gestureAssoc ?? null,
            gestureSettledAssoc: opts.gestureSettledAssoc ?? null,
            operationSettled: opts.operationSettled ?? [],
            nativeCreations: opts.nativeCreations ?? [],
            warpCursor: null,
            intentDesired: opts.intentDesired ?? [],
            switchSettled: [],
            diagnostics: [],
            source: body.source,
            nativeInvalid: body.nativeInvalid,
            clock: "1",
            presentation: DISABLED_PRESENTATION,
        });
        assert.ok(parseWorldReply(text) !== null, "nextReply must be valid v4 WorldReply");
        return text;
    };

    const runTimers = (maxDelay = 200): void => {
        let ran = true;
        while (ran) {
            ran = false;
            for (const entry of timers.splice(0)) {
                if (!entry.cancelled && entry.delay <= maxDelay) {
                    ran = true;
                    entry.cb();
                } else if (!entry.cancelled) {
                    timers.push(entry);
                }
            }
        }
    };

    return {
        handle,
        calls,
        getWaitWorld: () => latestWaitWorld,
        gestureCalls,
        presentationCalls,
        logs,
        shortcuts,
        geometryWrites,
        windowActivatedHandlers,
        winA,
        settleHandshake,
        nextReply,
        runTimers,
    };
}

describe("dev fixture-only deterministic omission and preview-reply skip", () => {
    it("strips dev controls from ordinary and installed builds", () => {
        const outDir = mkdtempSync(join(tmpdir(), "omnitiler-dev-strip-"));
        try {
            const ordinaryOut = join(outDir, "ordinary.js");
            const installedOut = join(outDir, "installed.js");
            execFileSync("npx", [
                "esbuild",
                "src/entry.ts",
                "--bundle",
                "--format=iife",
                "--target=es2017",
                "--minify-syntax",
                "--define:__OMNITILER_TRACE__=\"0\"",
                "--define:__OMNITILER_SCOPE_CLASS__=\"OmniTilerTestWindow\"",
                `--outfile=${ordinaryOut}`,
            ], { cwd: process.cwd(), stdio: "pipe" });
            execFileSync("npx", [
                "esbuild",
                "src/entry.ts",
                "--bundle",
                "--format=iife",
                "--target=es2017",
                "--minify-syntax",
                "--define:__OMNITILER_TRACE__=\"0\"",
                "--define:__OMNITILER_SCOPE_CLASS__=\"\"",
                "--define:__OMNITILER_SOURCE_REV__=\"test\"",
                `--outfile=${installedOut}`,
            ], { cwd: process.cwd(), stdio: "pipe" });

            for (const file of [ordinaryOut, installedOut]) {
                const bundle = readFileSync(file, "utf8");
                assert.doesNotMatch(bundle, /omnitiler-dev-omit-next-gesture-ingress/);
                assert.doesNotMatch(bundle, /omnitiler-dev-skip-next-preview-reply/);
                assert.doesNotMatch(bundle, /omnitiler-dev-skip-next-world-reply/);
                assert.doesNotMatch(bundle, /dev-omit-gesture-ingress/);
                assert.doesNotMatch(bundle, /dev-skip-preview-reply/);
                assert.doesNotMatch(bundle, /dev-skip-world-reply/);
                assert.doesNotMatch(bundle, /describe-receipt/);
                assert.doesNotMatch(bundle, /describe-window/);
                assert.doesNotMatch(bundle, /describe-op/);
                assert.doesNotMatch(bundle, /noteFixtureDescribeReceipt/);
            }
        } finally {
            rmSync(outDir, { recursive: true, force: true });
        }
    });

    it("rejects empty, non-fixture, and mixed scopes even in trace builds", () => {
        for (const badScope of [
            undefined,
            "",
            [],
            "kwrite",
            ["kwrite"],
            "OmniTilerTestWindow,kwrite",
            ["OmniTilerTestWindow", "kwrite"],
            ["kwrite", "OmniTilerTestWindow"],
        ]) {
            const h = makeHarness(badScope);
            try {
                assert.ok(h.handle !== null);
                assert.equal(h.shortcuts.has("omnitiler-dev-omit-next-gesture-ingress"), false, `rejected for ${String(badScope)}`);
                assert.equal(h.shortcuts.has("omnitiler-dev-skip-next-preview-reply"), false, `rejected for ${String(badScope)}`);
                assert.equal(h.shortcuts.has("omnitiler-dev-skip-next-world-reply"), false, `rejected for ${String(badScope)}`);
            } finally {
                h.handle?.stop();
            }
        }

        const good = makeHarness("OmniTilerTestWindow");
        try {
            assert.ok(good.handle !== null);
            const omitAction = good.shortcuts.get("omnitiler-dev-omit-next-gesture-ingress");
            const skipAction = good.shortcuts.get("omnitiler-dev-skip-next-preview-reply");
            const skipWorldAction = good.shortcuts.get("omnitiler-dev-skip-next-world-reply");
            assert.ok(omitAction !== undefined && omitAction.sequence === "");
            assert.ok(skipAction !== undefined && skipAction.sequence === "");
            assert.ok(skipWorldAction !== undefined && skipWorldAction.sequence === "");
        } finally {
            good.handle?.stop();
        }
    });

    it("omits nativeGesture on one genuine capture with raw witness, auto-disarms, and cleans up on gen reset / stop", () => {
        const h = makeHarness("OmniTilerTestWindow");
        try {
            h.settleHandshake("gen-1");
            h.runTimers();
            const omitAction = h.shortcuts.get("omnitiler-dev-omit-next-gesture-ingress");
            assert.ok(omitAction !== undefined);

            // Settle initial bootstrap DescribeWorld before arming.
            const initDesc = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(initDesc !== undefined);
            initDesc.callback(h.nextReply(initDesc.payload));
            h.calls.length = 0;

            // Arm omission before any gesture snapshot exists.
            omitAction.cb();
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-omit-gesture-ingress state=armed gen=gen-1"));

            // Capture without gestureSnapshot must NOT consume the one-shot.
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const capNoGesture = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(capNoGesture !== undefined);
            assert.equal((JSON.parse(capNoGesture.payload) as { report: { nativeGesture: unknown } }).report.nativeGesture, null);
            assert.equal(h.logs.some((l) => l.includes("ev=dev-omit-gesture-ingress state=consumed")), false);
            capNoGesture.callback(h.nextReply(capNoGesture.payload));
            h.calls.length = 0;

            // Deliver genuine native gesture snapshot ("updated").
            const waitCall = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(waitCall !== undefined);
            waitCall.callback(gestureSnapshotJson({ phase: "updated", serial: "7", revision: "3" }));
            h.runTimers();

            const omittedCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(omittedCap !== undefined);
            const omittedBody = JSON.parse(omittedCap.payload) as {
                report: { nativeGesture: unknown; windows: unknown[] };
            };
            assert.equal(omittedBody.report.nativeGesture, null, "nativeGesture omitted on armed capture");
            assert.equal(omittedBody.report.windows.length, 1, "real capture continues with windows");
            assert.ok(
                h.logs.includes(
                    "omnitiler:world:trace ev=dev-omit-gesture-ingress state=consumed " +
                    "gen=gen-1 instance=gesture-1 serial=7 rev=3 phase=updated win=win-a",
                ),
            );
            omittedCap.callback(h.nextReply(omittedCap.payload));
            h.calls.length = 0;

            // Next genuine capture carries the real nativeGesture (auto-disarmed).
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const nextCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(nextCap !== undefined);
            const nextBody = JSON.parse(nextCap.payload) as { report: { nativeGesture: { serial: string } | null } };
            assert.equal(nextBody.report.nativeGesture?.serial, "7");
            nextCap.callback(h.nextReply(nextCap.payload));
            h.calls.length = 0;

            // Generation reset clears armed omission.
            omitAction.cb();
            const waitWorld = h.getWaitWorld();
            assert.ok(waitWorld !== null);
            waitWorld.callback(JSON.stringify({
                v: 4,
                outcome: "rejected",
                kind: "generation-mismatch",
                message: "stale",
                gen: "gen-2",
                source: "1",
                nativeInvalid: "1",
                clock: "1",
                gestureAssoc: null,
                gestureSettledAssoc: null,
            }));
            h.settleHandshake("gen-2");
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-omit-gesture-ingress state=cleared reason=generation-reset"));

            // Stop clears armed omission.
            omitAction.cb();
            h.handle?.stop();
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-omit-gesture-ingress state=cleared reason=stop"));
        } finally {
            h.handle?.stop();
        }
    });

    it("skips one real non-null preview reply with witness, preserves preview-only + terminal/result, ignores null, and cleans up", () => {
        const h = makeHarness("OmniTilerTestWindow");
        try {
            h.settleHandshake("gen-1");
            h.runTimers();
            const skipAction = h.shortcuts.get("omnitiler-dev-skip-next-preview-reply");
            assert.ok(skipAction !== undefined);

            // Settle initial bootstrap.
            const initDesc = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(initDesc !== undefined);
            initDesc.callback(h.nextReply(initDesc.payload));
            h.calls.length = 0;
            h.presentationCalls.length = 0;
            h.geometryWrites.length = 0;

            // Arm preview reply skip.
            skipAction.cb();
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-skip-preview-reply state=armed gen=gen-1"));

            // Null dragPreview reply must NOT consume the one-shot.
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const nullCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(nullCap !== undefined);
            nullCap.callback(h.nextReply(nullCap.payload, { dragPreview: null, winRect: [10, 10, 200, 200] }));
            assert.equal(h.logs.some((l) => l.includes("ev=dev-skip-preview-reply state=consumed")), false);
            assert.equal(h.geometryWrites.length, 1, "null-preview reply still applies normally");
            h.geometryWrites.length = 0;
            h.calls.length = 0;
            h.presentationCalls.length = 0;

            // Queue a command + deliver a terminal gesture so we verify command-result collection
            // and terminal settlement discharge are preserved on the skipped reply!
            const focusLeft = h.shortcuts.get("omnitiler-focus-left");
            assert.ok(focusLeft !== undefined);
            focusLeft.cb();
            const queueCall = h.calls.find((c) => c.method === "QueueWorldCommand");
            assert.ok(queueCall !== undefined);
            const qBody = JSON.parse(queueCall.payload) as { client: string; clientCmd: string };
            queueCall.callback(JSON.stringify({
                v: 4, outcome: "queued", gen: "gen-1", client: qBody.client, clientCmd: qBody.clientCmd, globalCmd: "1",
            }));
            h.calls.length = 0;

            const waitGesture = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(waitGesture !== undefined);
            waitGesture.callback(gestureSnapshotJson({ phase: "released", serial: "9", revision: "4" }));
            h.runTimers();

            const previewCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(previewCap !== undefined);
            const previewReq = (JSON.parse(previewCap.payload) as { source: string }).source;
            previewCap.callback(h.nextReply(previewCap.payload, {
                dragPreview: {
                    id: "1",
                    rev: "3",
                    domainOut: "out",
                    domainWs: "ws1",
                    sourceOut: "out",
                    sourceWs: "ws1",
                    rect: [40, 50, 300, 400],
                    hover: null,
                    win: "win-a",
                },
                winRect: [99, 99, 200, 200],
                gestureSettled: { id: "9", committed: true },
                gestureAssoc: { instance: "gesture-1", serial: "9", coreId: "9" },
                gestureSettledAssoc: { instance: "gesture-1", serial: "9", coreId: "9" },
            }));

            // Verify TakeCommandResult was called and answer it.
            const takeCall = h.calls.find((c) => c.method === "TakeCommandResult");
            assert.ok(takeCall !== undefined, "command-result collection preserved on skipped preview reply");
            takeCall.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));

            // Verify actuator path was skipped (no geometry writes), preview-only ReadWorldPresentation ran,
            // terminal AckGesture ran, and exact witness was logged.
            assert.equal(h.geometryWrites.length, 0, "skipped preview reply runs zero geometry writes");
            assert.ok(h.presentationCalls.some((c) => c.method === "ReadWorldPresentation"), "preview-only ReadWorldPresentation preserved");
            const acks = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(acks.length, 1, "terminal settlement discharge preserved");
            assert.ok(
                h.logs.includes(
                    `omnitiler:world:trace ev=reply-skipped stage=dev-skip gen=gen-1 req=${previewReq} rev=3 obs=3 reason=dev-skip-preview-reply`,
                ),
            );
            assert.ok(
                h.logs.includes(
                    `omnitiler:world:trace ev=dev-skip-preview-reply state=consumed gen=gen-1 req=${previewReq} rev=3 obs=3 previewId=1 previewRev=3 rect=40,50,300,400 win=win-a`,
                ),
            );
            h.calls.length = 0;

            // Next non-null preview reply applies normally (one-shot auto-disarmed).
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const afterCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(afterCap !== undefined);
            afterCap.callback(h.nextReply(afterCap.payload, {
                dragPreview: {
                    id: "2",
                    rev: "4",
                    domainOut: "out",
                    domainWs: "ws1",
                    sourceOut: "out",
                    sourceWs: "ws1",
                    rect: [45, 55, 300, 400],
                    hover: null,
                    win: "win-a",
                },
                winRect: [88, 88, 200, 200],
            }));
            const afterTake = h.calls.find((c) => c.method === "TakeCommandResult");
            assert.ok(afterTake !== undefined);
            afterTake.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
            assert.equal(h.geometryWrites.length, 1, "subsequent non-null preview reply applies normally");

            // Generation reset and stop cleanup.
            skipAction.cb();
            const waitWorld = h.getWaitWorld();
            assert.ok(waitWorld !== null);
            waitWorld.callback(JSON.stringify({
                v: 4,
                outcome: "rejected",
                kind: "generation-mismatch",
                message: "stale",
                gen: "gen-2",
                source: "1",
                nativeInvalid: "1",
                clock: "1",
                gestureAssoc: null,
                gestureSettledAssoc: null,
            }));
            h.settleHandshake("gen-2");
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-skip-preview-reply state=cleared reason=generation-reset"));

            skipAction.cb();
            h.handle?.stop();
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-skip-preview-reply state=cleared reason=stop"));
        } finally {
            h.handle?.stop();
        }
    });

    it("omits one fresh nonsuperseded accepted world reply before intent incorporation and native apply, and persists on later commandless projection", () => {
        const h = makeHarness("OmniTilerTestWindow", { floatIntent: true });
        try {
            h.settleHandshake("gen-1");
            h.runTimers();
            const skipWorldAction = h.shortcuts.get("omnitiler-dev-skip-next-world-reply");
            assert.ok(skipWorldAction !== undefined);

            // Settle initial bootstrap.
            const initDesc = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(initDesc !== undefined);
            initDesc.callback(h.nextReply(initDesc.payload));
            h.calls.length = 0;
            h.geometryWrites.length = 0;

            // Arm world reply skip.
            skipWorldAction.cb();
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-skip-world-reply state=armed gen=gen-1"));

            // Superseded-during-flight reply must NOT consume the one-shot.
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const supersededCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(supersededCap !== undefined);
            h.windowActivatedHandlers[0]?.(h.winA);
            supersededCap.callback(h.nextReply(supersededCap.payload, { winRect: [15, 15, 200, 200] }));
            assert.equal(h.logs.some((l) => l.includes("ev=dev-skip-world-reply state=consumed")), false);
            h.calls.length = 0;
            h.runTimers();
            const followupCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(followupCap !== undefined);
            followupCap.callback(h.nextReply(followupCap.payload, { winRect: [16, 16, 200, 200] }));
            assert.equal(h.logs.some((l) => l.includes("ev=dev-skip-world-reply state=consumed")), true);
            h.logs.length = 0;
            h.calls.length = 0;
            h.geometryWrites.length = 0;

            // Now arm for the main retained-intent omission scenario.
            skipWorldAction.cb();
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-skip-world-reply state=armed gen=gen-1"));

            // Queue a command + deliver a terminal gesture so we verify command-result collection
            // and terminal settlement discharge are preserved on the omitted world reply.
            const focusLeft = h.shortcuts.get("omnitiler-focus-left");
            assert.ok(focusLeft !== undefined);
            focusLeft.cb();
            const queueCall = h.calls.find((c) => c.method === "QueueWorldCommand");
            assert.ok(queueCall !== undefined);
            const qBody = JSON.parse(queueCall.payload) as { client: string; clientCmd: string };
            const waitGesture = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(waitGesture !== undefined);
            waitGesture.callback(gestureSnapshotJson({ phase: "released", serial: "9", revision: "4" }));
            h.calls.length = 0;
            queueCall.callback(JSON.stringify({
                v: 4, outcome: "queued", gen: "gen-1", client: qBody.client, clientCmd: qBody.clientCmd, globalCmd: "1",
            }));
            h.runTimers();

            const omittedCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(omittedCap !== undefined);
            const omittedReq = (JSON.parse(omittedCap.payload) as { source: string }).source;
            const retainedData = "{\"v\":1,\"float\":true,\"stickyPrior\":null}";
            omittedCap.callback(h.nextReply(omittedCap.payload, {
                intentDesired: [{ win: "win-a", data: retainedData }],
                winRect: [50, 60, 300, 400],
                gestureSettled: { id: "9", committed: true },
                gestureAssoc: { instance: "gesture-1", serial: "9", coreId: "9" },
                gestureSettledAssoc: { instance: "gesture-1", serial: "9", coreId: "9" },
            }));

            // Verify TakeCommandResult was called and answer it.
            const takeCall = h.calls.find((c) => c.method === "TakeCommandResult");
            assert.ok(takeCall !== undefined, "command-result collection preserved on omitted world reply");
            takeCall.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));

            // Zero WriteFloatIntent and zero native application!
            assert.equal(h.calls.filter((c) => c.method === "WriteFloatIntent").length, 0, "omitted reply issues zero WriteFloatIntent");
            assert.equal(h.geometryWrites.length, 0, "omitted reply runs zero geometry writes");
            const acks = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(acks.length, 1, "terminal settlement discharge preserved");
            assert.ok(
                h.logs.some((l) => l.startsWith(`omnitiler:world:trace ev=reply-skipped stage=dev-skip gen=gen-1 req=${omittedReq} `) && l.endsWith(" reason=dev-skip-world-reply")),
                "reply-skipped witness logged",
            );
            const consumedLog = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=dev-skip-world-reply state=consumed gen=gen-1 req=${omittedReq} `));
            assert.ok(consumedLog !== undefined, "consumed witness logged");
            assert.match(consumedLog, /rev=\d+ obs=\d+ intentRows=1$/);
            assert.equal(consumedLog.includes("stickyPrior"), false, "no raw opaque bytes in witness");
            h.calls.length = 0;

            // Later commandless same retained projection causes exactly one WriteFloatIntent before apply.
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const persistCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(persistCap !== undefined, "flight was released so later commandless capture dispatches");
            persistCap.callback(h.nextReply(persistCap.payload, {
                intentDesired: [{ win: "win-a", data: retainedData }],
                winRect: [88, 88, 200, 200],
            }));
            const persistTake = h.calls.find((c) => c.method === "TakeCommandResult");
            assert.ok(persistTake !== undefined);
            persistTake.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
            const writesAfterProjection = h.calls.filter((c) => c.method === "WriteFloatIntent");
            assert.equal(writesAfterProjection.length, 1, "later commandless same retained projection causes exactly one WriteFloatIntent");
            assert.equal(h.geometryWrites.length, 0, "native apply waits until WriteFloatIntent completes");
            const writeCall = writesAfterProjection[0];
            assert.ok(writeCall !== undefined);
            const writeBody = JSON.parse(writeCall.payload) as { correlation_id: string; records: Array<{ win: string; data: string }> };
            assert.deepEqual(writeBody.records, [{ win: "win-a", data: retainedData }]);
            writeCall.callback(JSON.stringify({ v: 2, correlation_id: writeBody.correlation_id, outcome: "stored", stored: 1 }));
            assert.deepEqual(h.geometryWrites, ["88,88,200,200"], "native apply runs after WriteFloatIntent completes");
            h.calls.length = 0;
            h.geometryWrites.length = 0;

            // Unchanged later publication causes zero WriteFloatIntent.
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const unchangedCap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(unchangedCap !== undefined);
            unchangedCap.callback(h.nextReply(unchangedCap.payload, {
                intentDesired: [{ win: "win-a", data: retainedData }],
                winRect: [99, 99, 200, 200],
            }));
            const unchangedTake = h.calls.find((c) => c.method === "TakeCommandResult");
            assert.ok(unchangedTake !== undefined);
            unchangedTake.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
            assert.equal(h.calls.filter((c) => c.method === "WriteFloatIntent").length, 0, "unchanged later publication causes zero WriteFloatIntent");
            assert.deepEqual(h.geometryWrites, ["99,99,200,200"]);

            // Generation reset and stop cleanup.
            skipWorldAction.cb();
            const waitWorld = h.getWaitWorld();
            assert.ok(waitWorld !== null);
            waitWorld.callback(JSON.stringify({
                v: 4,
                outcome: "rejected",
                kind: "generation-mismatch",
                message: "stale",
                gen: "gen-2",
                source: "1",
                nativeInvalid: "1",
                clock: "1",
                gestureAssoc: null,
                gestureSettledAssoc: null,
            }));
            h.settleHandshake("gen-2");
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-skip-world-reply state=cleared reason=generation-reset"));

            skipWorldAction.cb();
            h.handle?.stop();
            assert.ok(h.logs.includes("omnitiler:world:trace ev=dev-skip-world-reply state=cleared reason=stop"));
        } finally {
            h.handle?.stop();
        }
    });

    it("suppresses Describe pending-operation diagnostic for empty, non-fixture, and mixed scopes in trace builds", () => {
        for (const badScope of [
            undefined,
            "",
            [],
            "kwrite",
            ["kwrite"],
            "OmniTilerTestWindow,kwrite",
            ["OmniTilerTestWindow", "kwrite"],
        ]) {
            const h = makeHarness(badScope);
            try {
                h.settleHandshake();
                const cap = h.calls.find((c) => c.method === "DescribeWorld");
                assert.ok(cap !== undefined);
                cap.callback(h.nextReply(cap.payload));
                assert.equal(h.logs.some((l) => l.includes("ev=describe-")), false, `suppressed for ${String(badScope)}`);
            } finally {
                h.handle?.stop();
            }
        }
    });

    it("logs correlated Describe window projection and all pending lanes for exact fixture scope without leaking intent bytes", () => {
        const h = makeHarness("OmniTilerTestWindow", { floatIntent: true });
        try {
            h.settleHandshake();
            const secretIntent = JSON.stringify({ v: 1, floating: true, rect: [11, 22, 333, 444], secretToken: "stickyPrior" });

            // 1. Zero projected targets: known-zero counts vs absent switch/gesture pending lanes.
            const cap0 = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(cap0 !== undefined);
            const req0 = (JSON.parse(cap0.payload) as { source: string }).source;
            cap0.callback(h.nextReply(cap0.payload, { winRect: [10, 20, 300, 400] }));
            const take0 = h.calls.find((c) => c.method === "TakeCommandResult");
            take0?.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));

            const receipt0 = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=describe-receipt gen=gen-1 req=${req0} `));
            assert.ok(receipt0 !== undefined, "zero-projected-targets describe-receipt logged");
            assert.match(receipt0, /rev=1 obs=1 .*windows=1 shownWindows=1 windowsOmitted=0 intentDesired=0 projectedTargets=0 frameOps=0 clearOps=0 stateTargets=0 aboveTargets=0 transfers=0 creations=0 focusReq=0 dragPreview=0 switchPending=absent gesturePending=absent/);
            const winRow0 = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=describe-window gen=gen-1 req=${req0} `));
            assert.ok(winRow0 !== undefined, "zero-pending describe-window logged");
            assert.match(winRow0, /rev=1 obs=1 win=win-a floating=0 floatOrigin=- wsMode=tiled out=out ws=wl-1 frame=10,20,300,400 alloc=10,20,300,400 .*frameOp=- clearOp=- cmdSeq=- stateOps=- xferOps=-$/);
            assert.deepEqual(h.geometryWrites, ["10,20,300,400"]);
            h.calls.length = 0;
            h.geometryWrites.length = 0;
            h.logs.length = 0;

            // 2. Pending above/state target on floating window.
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const capAbove = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(capAbove !== undefined);
            const reqAbove = (JSON.parse(capAbove.payload) as { source: string }).source;
            capAbove.callback(h.nextReply(capAbove.payload, {
                wsMode: "floating",
                windows: [{
                    win: "win-a",
                    out: "out",
                    ws: "wl-1",
                    alloc: null,
                    frame: [30, 40, 250, 260],
                    frame_auth: "controlled",
                    frame_hold: "",
                    member_auth: "controlled",
                    member_hold: "",
                    frame_op: "",
                    clear_op: "",
                    floating: true,
                    float_origin: "",
                    cmdSeq: null,
                    visibility: null,
                    visibility_auth: "untouched",
                    visibility_hold: "",
                    state: [{ property: "above", value: true, auth: "controlled", reason: "", cmdSeq: "42" }],
                }],
            }));
            const takeAbove = h.calls.find((c) => c.method === "TakeCommandResult");
            takeAbove?.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));

            const receiptAbove = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=describe-receipt gen=gen-1 req=${reqAbove} `));
            assert.ok(receiptAbove !== undefined);
            assert.match(receiptAbove, /rev=2 obs=2 .*projectedTargets=1 frameOps=0 clearOps=0 stateTargets=1 aboveTargets=1 transfers=0 creations=0 focusReq=0 dragPreview=0 switchPending=absent gesturePending=absent/);
            const winRowAbove = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=describe-window gen=gen-1 req=${reqAbove} `));
            assert.ok(winRowAbove !== undefined);
            assert.match(winRowAbove, /win=win-a floating=1 floatOrigin=- wsMode=floating out=out ws=wl-1 frame=30,40,250,260 alloc=- .*frameOp=- clearOp=- cmdSeq=- stateOps=above=1@controlled:-#42 xferOps=-$/);
            h.calls.length = 0;
            h.geometryWrites.length = 0;
            h.logs.length = 0;

            // 3. Pending frame_op, clear_op, transfer, creation, focusRequest, dragPreview + before-skip receipt + intentDesired count-only.
            const skipWorldAction = h.shortcuts.get("omnitiler-dev-skip-next-world-reply");
            assert.ok(skipWorldAction !== undefined);
            skipWorldAction.cb();
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const capMulti = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(capMulti !== undefined);
            const reqMulti = (JSON.parse(capMulti.payload) as { source: string }).source;
            capMulti.callback(h.nextReply(capMulti.payload, {
                intentDesired: [{ win: "win-a", data: secretIntent }],
                windows: [{
                    win: "win-a",
                    out: "out",
                    ws: "wl-1",
                    alloc: null,
                    frame: [50, 60, 220, 240],
                    frame_auth: "controlled",
                    frame_hold: "",
                    member_auth: "controlled",
                    member_hold: "",
                    frame_op: "op-frame-1",
                    clear_op: "op-clear-2",
                    floating: true,
                    float_origin: "capacity",
                    cmdSeq: "99",
                    visibility: null,
                    visibility_auth: "untouched",
                    visibility_hold: "",
                    state: [],
                }],
                transfers: [{ op: "op-xfer-3", kind: "window", win: "win-a", out: "out", ws: "wl-1", follow: true, held: false, cmdSeq: "100" }],
                nativeCreations: [{ op: "op-create-4", ws: "wl-2", out: "out" }],
                focusRequest: { id: "focus-5", target: "win-a", rev: "3", cause: "explicit-command", out: "out", ws: "wl-1", cmdSeq: "101", afterShown: null },
                dragPreview: { id: "6", rev: "3", domainOut: "out", domainWs: "wl-1", sourceOut: "out", sourceWs: "wl-1", rect: [1, 2, 30, 40], hover: null, win: "win-a" },
            }));
            const takeMulti = h.calls.find((c) => c.method === "TakeCommandResult");
            takeMulti?.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));

            const receiptMulti = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=describe-receipt gen=gen-1 req=${reqMulti} `));
            assert.ok(receiptMulti !== undefined, "describe-receipt emitted before dev-skip");
            assert.match(receiptMulti, /rev=3 obs=3 .*intentDesired=1 projectedTargets=6 frameOps=1 clearOps=1 stateTargets=0 aboveTargets=0 transfers=1 creations=1 focusReq=1 dragPreview=1 switchPending=absent gesturePending=absent/);
            assert.ok(h.logs.some((l) => l.startsWith(`omnitiler:world:trace ev=settlement-receipt gen=gen-1 req=${reqMulti} rev=3 obs=3 `)));
            assert.ok(h.logs.some((l) => l.startsWith(`omnitiler:world:trace ev=reply-skipped stage=dev-skip gen=gen-1 req=${reqMulti} rev=3 obs=3 `)));
            const winRowMulti = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=describe-window gen=gen-1 req=${reqMulti} `));
            assert.ok(winRowMulti !== undefined);
            assert.match(winRowMulti, /floatOrigin=capacity .*frameOp=op-frame-1 clearOp=op-clear-2 cmdSeq=99 stateOps=- xferOps=window:op-xfer-3:active#100$/);
            assert.ok(h.logs.some((l) => l.includes(`ev=describe-op lane=transfer gen=gen-1 req=${reqMulti} rev=3 obs=3 op=op-xfer-3 kind=window win=win-a`)));
            assert.ok(h.logs.some((l) => l.includes(`ev=describe-op lane=creation gen=gen-1 req=${reqMulti} rev=3 obs=3 op=op-create-4 out=out ws=wl-2`)));
            assert.ok(h.logs.some((l) => l.includes(`ev=describe-op lane=focus gen=gen-1 req=${reqMulti} rev=3 obs=3 op=focus-5 target=win-a`)));
            assert.ok(h.logs.some((l) => l.includes(`ev=describe-op lane=drag-preview gen=gen-1 req=${reqMulti} rev=3 obs=3 op=6 win=win-a`)));
            assert.equal(h.logs.some((l) => l.includes("stickyPrior")), false, "no raw opaque intent bytes logged");
            assert.equal(h.geometryWrites.length, 0, "dev-skip still skips geometry writes");
            h.calls.length = 0;
            h.logs.length = 0;

            // 4. Subsequent reply for same lifetime with cleared wire target projections shows projectedTargets=0.
            h.windowActivatedHandlers[0]?.(h.winA);
            h.runTimers();
            const capDone = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(capDone !== undefined);
            const reqDone = (JSON.parse(capDone.payload) as { source: string }).source;
            capDone.callback(h.nextReply(capDone.payload, { winRect: [50, 60, 220, 240] }));
            const takeDone = h.calls.find((c) => c.method === "TakeCommandResult");
            takeDone?.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
            const receiptDone = h.logs.find((l) => l.startsWith(`omnitiler:world:trace ev=describe-receipt gen=gen-1 req=${reqDone} `));
            assert.ok(receiptDone !== undefined);
            assert.match(receiptDone, /rev=4 obs=4 .*projectedTargets=0 /);
        } finally {
            h.handle?.stop();
        }
    });

    it("fails open if Describe diagnostic logging throws without affecting setters", () => {
        const h = makeHarness("OmniTilerTestWindow", {
            onLog: (line) => {
                if (line.includes("ev=describe-")) {
                    throw new Error("simulated-log-failure");
                }
            },
        });
        try {
            h.settleHandshake();
            const cap = h.calls.find((c) => c.method === "DescribeWorld");
            assert.ok(cap !== undefined);
            cap.callback(h.nextReply(cap.payload, { winRect: [77, 88, 210, 220] }));
            const take = h.calls.find((c) => c.method === "TakeCommandResult");
            take?.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
            assert.deepEqual(h.geometryWrites, ["77,88,210,220"], "setters still execute when diagnostic throws");
        } finally {
            h.handle?.stop();
        }
    });
});
