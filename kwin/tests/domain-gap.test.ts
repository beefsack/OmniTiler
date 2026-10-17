import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import {
    DOMAIN_GAP,
    OUTER_DOMAIN_GAP,
    normalizeGap,
    readDomainGaps,
    readInnerGapValue,
    readOuterGapValue,
} from "../src/domain-gap";
import { startPlanAdapterEntry } from "../src/plan-adapter-entry";
import {
    WorkspaceSendAdapter,
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    type WorkspaceSendAdapterEnv,
    type WorkspaceSendObserved,
} from "../src/workspace-send-adapter";

function entryXml(path: string): string {
    return readFileSync(path, "utf8");
}

describe("bounded gap schema and native KCM persistence", () => {
    it("declares innerGap and outerGap as bounded Int settings defaulting to 8", () => {
        const schema = entryXml("contents/config/main.xml");
        for (const key of ["innerGap", "outerGap"]) {
            const match = schema.match(
                new RegExp(`<entry name="${key}" type="Int">([\\s\\S]*?)</entry>`),
            );
            assert.ok(match !== null, `${key} must be declared`);
            const body = match[1] as string;
            assert.match(body, /<default>8<\/default>/);
            assert.match(body, /<min>0<\/min>/);
            assert.match(body, /<max>64<\/max>/);
        }
        const ui = entryXml("contents/ui/config.ui");
        for (const key of ["innerGap", "outerGap"]) {
            assert.match(ui, new RegExp(`name="kcfg_${key}"`));
        }
    });

    it("persists gaps through the native script KCM with bounded validation", () => {
        const module = entryXml("native-effect/unifiedsettings_module.cpp");
        const adapter = entryXml("native-effect/kdesettings_adapter.cpp");
        const descriptors = entryXml("native-effect/generated/kde_settings_descriptors.h");
        const ui = entryXml("native-effect/unifiedsettings.ui");
        for (const key of ["innerGap", "outerGap"]) {
            assert.match(descriptors, new RegExp(`"${key}", "${key}SpinBox"`));
            assert.match(descriptors, new RegExp(`"8", "0", "64"`));
            assert.match(adapter, /readBoundedInt/);
            assert.match(adapter, /isBoundedIntRawValid/);
            assert.match(module, /KdeSettingsAdapter::readBoundedInt\(/);
            assert.match(module, new RegExp(`writeEntry\\(QStringLiteral\\("${key}"\\)`));
            assert.match(ui, new RegExp(`name="${key}SpinBox"`));
            assert.match(ui, new RegExp(`name="label_${key}"[\\s\\S]*?${key}SpinBox`));
        }
        assert.match(module, /m_ui\.innerGapSpinBox->setValue\(innerGap\)/);
        assert.match(module, /m_ui\.outerGapSpinBox->setValue\(outerGap\)/);
        assert.match(module, /KdeSettingsAdapter::configureNativeControls\(widget\(\)\)/);
        assert.match(adapter, /configureBoundedSpinBox/);
        assert.match(ui, /name="innerGapSpinBox"[\s\S]*?<number>64<\/number>/);
        assert.match(ui, /name="outerGapSpinBox"[\s\S]*?<number>64<\/number>/);
    });
});

describe("validated gap binding", () => {
    it("preserves the default effective pair (8, 8)", () => {
        assert.equal(DOMAIN_GAP, 8);
        assert.equal(OUTER_DOMAIN_GAP, 8);
        assert.deepEqual(readDomainGaps(), { innerGap: 8, outerGap: 8 });
    });

    it("accepts boundaries and representative values while rejecting the rest to 8", () => {
        assert.equal(normalizeGap(0), 0);
        assert.equal(normalizeGap(64), 64);
        assert.equal(normalizeGap(12), 12);
        assert.equal(normalizeGap("20"), 20);
        assert.equal(normalizeGap(-1), 8);
        assert.equal(normalizeGap(65), 8);
        assert.equal(normalizeGap(7.5), 8);
        assert.equal(normalizeGap("bogus"), 8);
        assert.equal(normalizeGap(undefined), 8);
        assert.equal(normalizeGap(null), 8);
        assert.equal(readInnerGapValue(() => 12), 12);
        assert.equal(readOuterGapValue(() => 20), 20);
        assert.equal(readInnerGapValue(() => 65), 8);
        assert.equal(readOuterGapValue(() => -1), 8);
        assert.equal(
            readInnerGapValue(() => {
                throw new Error("boom");
            }),
            8,
        );
        assert.deepEqual(readDomainGaps({ readInnerGapFn: () => 0, readOuterGapFn: () => 64 }), {
            innerGap: 0,
            outerGap: 64,
        });
        assert.deepEqual(readDomainGaps({ readInnerGapFn: () => "bogus", readOuterGapFn: () => 99 }), {
            innerGap: 8,
            outerGap: 8,
        });
    });
});

function planWorld(): Record<string, unknown> {
    const output: Record<string, unknown> = { name: "out-1" };
    const desktop: Record<string, unknown> = { id: "ws-1" };
    const inert = (): unknown => ({ connect: (): void => {}, disconnect: (): void => {} });
    const winA: Record<string, unknown> = {
        normalWindow: true,
        internalId: "win-a",
        resourceClass: "test-app",
        output,
        desktops: [desktop],
        frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
        moveResizedChanged: inert(),
        fullScreenChanged: inert(),
        fullScreen: false,
        maximizedChanged: inert(),
        maximizeMode: 0,
        desktopsChanged: inert(),
        onAllDesktops: false,
    };
    const winB: Record<string, unknown> = {
        normalWindow: true,
        internalId: "win-b",
        resourceClass: "test-app",
        output,
        desktops: [desktop],
        frameGeometry: { x: 600, y: 0, width: 600, height: 800 },
        moveResizedChanged: inert(),
        fullScreenChanged: inert(),
        fullScreen: false,
        maximizedChanged: inert(),
        maximizeMode: 0,
        desktopsChanged: inert(),
        onAllDesktops: false,
    };
    return {
        output,
        desktop,
        winA,
        winB,
        workspace: {
            activeWindow: winA,
            windowList: (): unknown[] => [winA, winB],
            screens: [output],
            currentDesktopForScreen: (): unknown => desktop,
            clientArea: (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowAdded: inert(),
            windowRemoved: inert(),
            windowActivated: inert(),
            screensChanged: inert(),
            currentDesktopChanged: inert(),
        },
    };
}

describe("production entries carry configured gaps", () => {
    it("normal plan observation sends configured gaps in DescribePlan", () => {
        const world = planWorld();
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const handle = startPlanAdapterEntry({
            workspace: world["workspace"],
            callDbus: (_s, _p, _i, method, payload, _cb): void => {
            if (method === "NameHasOwner") { _cb(true); return; }
            if (method === "GetNameOwner") { _cb(":1.7"); return; }
            if (method === "StartServiceByName") { _cb(1); return; }
                dbusCalls.push({ method, payload });
                callbacks.push(_cb);
            },
            scheduleOnce: () => () => {},
            log: () => {},
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: () => true,
            readProfileFn: () => "cosmic",
            readInnerGapFn: () => 12,
            readOuterGapFn: () => 20,
        });
        assert.ok(handle !== null);
        handle?.requestFocus("left");
        assert.equal(dbusCalls.length, 1);
        const payload = JSON.parse(dbusCalls[0]?.payload as string) as Record<string, unknown>;
        const domain = payload["domain"] as Record<string, unknown>;
        assert.equal(domain["gap"], 12);
        assert.equal(domain["outer_gap"], 20);
        handle?.stop();
    });

    it("invalid plan gaps fall back to the default pair", () => {
        const world = planWorld();
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const handle = startPlanAdapterEntry({
            workspace: world["workspace"],
            callDbus: (_s, _p, _i, method, payload, _cb): void => {
            if (method === "NameHasOwner") { _cb(true); return; }
            if (method === "GetNameOwner") { _cb(":1.7"); return; }
            if (method === "StartServiceByName") { _cb(1); return; }
                dbusCalls.push({ method, payload });
                void _cb;
            },
            scheduleOnce: () => () => {},
            log: () => {},
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: () => true,
            readProfileFn: () => "cosmic",
            readInnerGapFn: () => 99,
            readOuterGapFn: () => "bogus",
        });
        assert.ok(handle !== null);
        handle?.requestFocus("left");
        const payload = JSON.parse(dbusCalls[0]?.payload as string) as Record<string, unknown>;
        const domain = payload["domain"] as Record<string, unknown>;
        assert.equal(domain["gap"], 8);
        assert.equal(domain["outer_gap"], 8);
        handle?.stop();
    });

    it("every workspace-send request builder carries configured gaps", () => {
        const refs = { a: {}, b: {}, t: {}, desktop: {} };
        const rect = (x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } => ({ x, y, w, h });
        const observed: WorkspaceSendObserved = {
            sourceOutput: "out-1",
            sourceWorkspace: "ws-1",
            sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
            targetOutput: "out-1",
            targetWorkspace: "ws-2",
            targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
            focusedId: "win-a",
            sourceWindows: Object.freeze([
                Object.freeze({ id: "win-a", ref: refs.a, rect: Object.freeze(rect(0, 0, 100, 100)) }),
                Object.freeze({ id: "win-b", ref: refs.b, rect: Object.freeze(rect(100, 0, 100, 100)) }),
            ]),
            targetWindows: Object.freeze([
                Object.freeze({ id: "win-t", ref: refs.t, rect: Object.freeze(rect(0, 0, 100, 100)) }),
            ]),
            activeRef: refs.a,
            moverRef: refs.a,
            targetDesktopRef: refs.desktop,
            targetExists: true,
            desktopCount: 2,
            sourceFingerprint: "sfp-1",
            targetFingerprint: "tfp-1",
            currentWorkspace: "ws-1",
        };
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const env: WorkspaceSendAdapterEnv = {
            callDbus: (_s, _p, _i, method, payload, cb): void => {
                if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                    cb(true);
                    return;
                }
                dbusCalls.push({ method, payload });
                callbacks.push(cb);
            },
            scheduleOnce: () => () => {},
            log: () => {},
            observe: () => observed,
            setGeometry: () => true,
            setDesktops: () => true,
        };
        const adapter = new WorkspaceSendAdapter(env, { innerGap: 12, outerGap: 20 });
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        assert.equal(adapter.requestSend("ws-2"), true);
        assert.ok(dbusCalls.length >= 1);
        callbacks[0]?.(":1.42");
        const requestCall = dbusCalls.find((call) => {
            try {
                const body = JSON.parse(call.payload) as Record<string, unknown>;
                return (body["command"] as Record<string, unknown>)?.["op"] === "send-to-workspace";
            } catch (error) {
                void error;
                return false;
            }
        });
        assert.ok(requestCall !== undefined);
        const requestBody = JSON.parse(requestCall.payload) as Record<string, unknown>;
        assert.equal((requestBody["domain"] as Record<string, unknown>)["gap"], 12);
        assert.equal((requestBody["domain"] as Record<string, unknown>)["outer_gap"], 20);
        assert.equal((requestBody["target_domain"] as Record<string, unknown>)["gap"], 12);
        assert.equal((requestBody["target_domain"] as Record<string, unknown>)["outer_gap"], 20);
        adapter.disable();
    });

    it("workspace-send defaults to (8, 8) and falls back on invalid gaps", () => {
        const refs = { a: {}, b: {}, t: {}, desktop: {} };
        const rect = (x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } => ({ x, y, w, h });
        const observed: WorkspaceSendObserved = {
            sourceOutput: "out-1",
            sourceWorkspace: "ws-1",
            sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
            targetOutput: "out-1",
            targetWorkspace: "ws-2",
            targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
            focusedId: "win-a",
            sourceWindows: Object.freeze([
                Object.freeze({ id: "win-a", ref: refs.a, rect: Object.freeze(rect(0, 0, 100, 100)) }),
            ]),
            targetWindows: Object.freeze([]),
            activeRef: refs.a,
            moverRef: refs.a,
            targetDesktopRef: refs.desktop,
            targetExists: true,
            desktopCount: 2,
            sourceFingerprint: "sfp-1",
            targetFingerprint: "tfp-1",
            currentWorkspace: "ws-1",
        };
        const runOnce = (gaps: { innerGap?: unknown; outerGap?: unknown } | undefined): Record<string, unknown> => {
            const dbusCalls: Array<{ method: string; payload: string }> = [];
            const callbacks: Array<(reply: unknown) => void> = [];
            const env: WorkspaceSendAdapterEnv = {
                callDbus: (_s, _p, _i, method, payload, cb): void => {
                    if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                        cb(true);
                        return;
                    }
                    dbusCalls.push({ method, payload });
                    callbacks.push(cb);
                },
                scheduleOnce: () => () => {},
                log: () => {},
                observe: () => observed,
                setGeometry: () => true,
                setDesktops: () => true,
            };
            const adapter = gaps === undefined ? new WorkspaceSendAdapter(env) : new WorkspaceSendAdapter(env, gaps);
            assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
            assert.equal(adapter.requestSend("ws-2"), true);
            callbacks[0]?.(":1.42");
            const requestCall = dbusCalls.find((call) => {
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    return (body["command"] as Record<string, unknown>)?.["op"] === "send-to-workspace";
                } catch (error) {
                    void error;
                    return false;
                }
            });
            assert.ok(requestCall !== undefined);
            adapter.disable();
            return JSON.parse((requestCall as { payload: string }).payload) as Record<string, unknown>;
        };
        const fallback = runOnce(undefined);
        assert.equal((fallback["domain"] as Record<string, unknown>)["gap"], 8);
        assert.equal((fallback["domain"] as Record<string, unknown>)["outer_gap"], 8);
        const invalid = runOnce({ innerGap: 99, outerGap: "bogus" });
        assert.equal((invalid["domain"] as Record<string, unknown>)["gap"], 8);
        assert.equal((invalid["domain"] as Record<string, unknown>)["outer_gap"], 8);
    });
});
