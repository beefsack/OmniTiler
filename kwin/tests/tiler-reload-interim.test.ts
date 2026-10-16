import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it } from "node:test";

import { startPlanAdapterEntry } from "../src/plan-adapter-entry";

const read = (path: string): string => readFileSync(join(process.cwd(), path), "utf8");

const module = read("native-effect/unifiedsettings_module.cpp");
const header = read("native-effect/unifiedsettings_module.h");
const ui = read("native-effect/unifiedsettings.ui");
const entry = read("src/plan-adapter-entry.ts");
const gaps = read("src/domain-gap.ts");
const sendAdapter = read("src/workspace-send-adapter.ts");
const productionEntry = read("src/entry.ts");

function functionBody(source: string, signature: string): string {
    const start = source.indexOf(signature);
    assert.notEqual(start, -1, `${signature} must exist`);
    const open = source.indexOf("{", start);
    assert.notEqual(open, -1, `${signature} must open a body`);
    let depth = 0;
    for (let index = open; index < source.length; index += 1) {
        const ch = source[index];
        if (ch === "{") {
            depth += 1;
        } else if (ch === "}") {
            depth -= 1;
            if (depth === 0) {
                return source.slice(open, index + 1);
            }
        }
    }
    assert.fail(`${signature} must close its body`);
}

interface FakeSignal {
    readonly handlers: Array<() => void>;
    readonly signal: { connect: (handler: () => void) => void; disconnect: (handler: () => void) => void };
}

function fakeSignal(): FakeSignal {
    const handlers: Array<() => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: () => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: () => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

function fakeWorkspace(): Record<string, unknown> {
    const output: Record<string, unknown> = { name: "out-1" };
    const desktop: Record<string, unknown> = { id: "ws-1" };
    const added = fakeSignal();
    const removed = fakeSignal();
    const activated = fakeSignal();
    const screensChanged = fakeSignal();
    const desktopChanged = fakeSignal();
    const makeWin = (id: string, x: number): Record<string, unknown> => {
        const geo = fakeSignal();
        const full = fakeSignal();
        const max = fakeSignal();
        const desktopsChanged = fakeSignal();
        const win: Record<string, unknown> = {
            normalWindow: true,
            internalId: id,
            resourceClass: "test-app",
            output,
            desktops: [desktop],
            frameGeometry: { x, y: 0, width: 600, height: 800 },
            frameGeometryChanged: geo.signal,
            fullScreenChanged: full.signal,
            fullScreen: false,
            maximizedChanged: max.signal,
            maximizeMode: 0,
            desktopsChanged: desktopsChanged.signal,
            onAllDesktops: false,
        };
        win["setMaximize"] = (): void => {};
        return win;
    };
    const winA = makeWin("win-a", 0);
    const winB = makeWin("win-b", 600);
    return {
        activeWindow: winA,
        windowList: (): unknown[] => [winA, winB],
        screens: [output],
        desktops: [desktop],
        currentDesktopForScreen: (): unknown => desktop,
        clientArea: (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 }),
        windowAdded: added.signal,
        windowRemoved: removed.signal,
        windowActivated: activated.signal,
        screensChanged: screensChanged.signal,
        currentDesktopChanged: desktopChanged.signal,
    };
}

interface ReloadMocks {
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly timers: Array<{ callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    readonly shortcuts: Array<{ action: string; sequence: string }>;
}

function domainOf(payload: string): { gap: unknown; outer_gap: unknown } {
    const parsed = JSON.parse(payload) as { domain?: { gap?: unknown; outer_gap?: unknown } };
    return { gap: parsed.domain?.gap, outer_gap: parsed.domain?.outer_gap };
}

describe("interim tiler reload contract", () => {
    it("keeps the production entry bound to the world adapter", () => {
        assert.match(productionEntry, /startWorldAdapterEntry/);
        assert.match(productionEntry, /owner: "kwin-world-adapter"/);
    });

    it("wires deliberate reload through Options configChanged with gap re-read and resync", () => {
        assert.match(entry, /Deliberate tiler reload gap configuration/);
        assert.match(entry, /options.*configChanged|configChanged.*options/);
        assert.match(entry, /readDomainGaps\(\{/);
        assert.match(entry, /requestResync\(\)/);
        assert.match(entry, /omnitiler:plan:config-reloaded/);
        assert.match(entry, /stage=re-read-queued/);
        assert.match(entry, /applied-unconfirmed/);
        assert.match(entry, /stage=restart-required/);
        assert.match(entry, /readShortcutProfile\(overrides\.readProfileFn\)/);
        assert.match(entry, /readWorkspaceModeValue\(overrides\.readWorkspaceModeFn\)/);
        // No script/plugin lifecycle or shortcut re-registration on the reload path.
        assert.doesNotMatch(entry, /loadScript/);
        assert.doesNotMatch(entry, /unloadScript/);
        assert.doesNotMatch(entry, /registerShortcutFn\(action/);
        // Provenance: the update-gaps plan flight runs the ordinary retained
        // route on the planner channel while workspace sends commit on the
        // separate send channel, so no send outcome may back an applied
        // claim. The entry keeps no outstanding pair and the send adapter
        // exposes no committed-pair API.
        assert.doesNotMatch(entry, /stage=applied/);
        assert.doesNotMatch(entry, /reloadOutstanding/);
        assert.doesNotMatch(entry, /evidence=committed-flight/);
        assert.doesNotMatch(sendAdapter, /committedGaps/);
    });

    it("types the deliberate reload as one KWin reconfigure send without claiming application", () => {
        assert.match(module, /QStringLiteral\("org\.kde\.KWin"\)/);
        assert.match(module, /QStringLiteral\("\/KWin"\)/);
        assert.match(module, /QStringLiteral\("org\.kde\.KWin"\)/);
        assert.match(module, /QStringLiteral\("reconfigure"\)/);
        assert.match(module, /Q_NOREPLY/);
        assert.match(module, /never proves the/);
        assert.match(module, /never claims the running tiler applied/);
        assert.match(header, /requestScriptReconfigure/);
        assert.match(header, /isScriptRestartRequired/);
        assert.match(header, /scriptStatusText/);
        assert.doesNotMatch(header, /requestTilerReload/);
        assert.doesNotMatch(header, /isTilerReloadRequired/);
        assert.doesNotMatch(header, /tilerReloadStatusText/);
        assert.doesNotMatch(module, /m_scriptReconfigurePending/);
        const reconfigureBody = functionBody(module, "bool UnifiedSettingsModule::requestScriptReconfigure()");
        assert.match(reconfigureBody, /\.send\(/);
        assert.doesNotMatch(reconfigureBody, /\.call\(/);
    });

    it("sends nothing on an unchanged save and requests reconfigure only for changed live settings", () => {
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        assert.match(saveBody, /scriptRetryArmed = m_gapReconfigurePending/);
        assert.match(saveBody, /const bool liveChanged = gapChanged \|\| sameAxisMoveChanged \|\| fixedPredicateChanged/);
        assert.match(saveBody, /if \(liveChanged \|\| scriptRetryArmed\)/);
        assert.match(saveBody, /requestScriptReconfigure\(\)/);
        assert.doesNotMatch(module, /setEnabled\(m_tilerReloadRequired\)/);
        assert.doesNotMatch(module, /tilerReloadButton/);
    });

    it("marks changed gaps for reconfigure and workspace mode restart-required with auto-send", () => {
        assert.match(module, /m_scriptRestartRequired = true/);
        assert.match(module, /gapChanged/);
        assert.match(module, /startupConsumedChanged/);
        assert.match(module, /Reconfigure request sent/);
        assert.match(module, /Session restart remains required/);
        assert.match(module, /m_gapReconfigurePending = true/);
        assert.match(module, /retry on the next save/);
        assert.doesNotMatch(module, /tilingAlgorithm|automaticSplitTarget|dropOutlinePreview|unconsumed settings/);
        assert.match(module, /startup gap values/);
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        assert.match(saveBody, /scriptRetryArmed = m_gapReconfigurePending/);
        assert.match(saveBody, /if \(liveChanged \|\| scriptRetryArmed\)/);
        assert.match(saveBody, /m_scriptRestartRequired = true/);
        assert.match(saveBody, /requestScriptReconfigure\(\)/);
        assert.match(gaps, /re-resolve/);
    });

    it("reports sent-but-unconfirmed and failed states without an applied claim and keeps restart residual", () => {
        assert.match(module, /Reconfigure request sent; application unconfirmed/);
        assert.match(module, /application unconfirmed/i);
        assert.match(module, /Reconfigure request failed; the running tiler still uses startup gap values/);
        assert.match(module, /restart the session to guarantee pickup/i);
        assert.match(module, /session restart remains required for workspace mode/i);
        assert.doesNotMatch(module, /No running tiler effect for unconsumed settings/);
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        const saveStrings = saveBody
            .split("\n")
            .filter((line) => line.includes("QStringLiteral"));
        assert.ok(saveStrings.length >= 2);
        for (const line of saveStrings) {
            assert.doesNotMatch(line, /applied/i);
        }
        const statusLines = module
            .split("\n")
            .filter((line) => line.includes("m_scriptStatus ="));
        assert.ok(statusLines.length >= 3);
        for (const line of statusLines) {
            assert.doesNotMatch(line, /applied/i);
        }
    });

    it("names both live keys in combined save status and keeps retry wording generic", () => {
        assert.match(module, /gaps and same-axis move/);
        assert.match(module, /Tiling gaps and same-axis move saved to kwinrc/);
        assert.match(module, /gaps and same-axis move is unconfirmed/);
        assert.match(module, /pending live settings/);
        assert.match(module, /pickup of pending live settings is unconfirmed/);
    });

    it("keeps shortcut mutation out of ordinary save", () => {
        const saveBody = functionBody(module, "void UnifiedSettingsModule::save()");
        for (const forbidden of [
            /ShortcutReconciler/,
            /KGlobalAccel/,
            /setShortcutKeys/,
            /globalShortcut/i,
            /confirmShortcutAction/,
            /runShortcutApply/,
        ]) {
            assert.doesNotMatch(saveBody, forbidden);
        }
    });

    it("exposes gap save status with restart residual in the unified dialog", () => {
        assert.match(ui, /name="scriptStatusLabel"/);
        assert.doesNotMatch(ui, /name="tilerReloadButton"/);
        assert.match(ui, /No pending script setting in this dialog\./);
        assert.match(ui, /Session restart is the guaranteed pickup mechanism/);
        assert.match(ui, /never claims the running tiler applied the settings/);
        assert.match(ui, /Saving changed gaps sends one typed KWin reconfigure request/);
        assert.match(ui, /workspace mode requires a session restart/i);
        assert.doesNotMatch(ui, /unconsumed settings have no running effect/i);
        assert.match(ui, /Border changes apply immediately through the KWin effect reconfigure\./);
        assert.match(ui, /Tiling gaps and workspace mode are on the Tiling &amp; Workspaces tab\./);
    });
});

describe("deliberate tiler reload behavior", () => {
    it("applies an altered gap setting to the reconfigured controller on configChanged", () => {
        let innerGap = 8;
        let outerGap = 8;
        const optionsChanged = fakeSignal();
        const mocks: ReloadMocks = { dbusCalls: [], callbacks: [], timers: [], logs: [], shortcuts: [] };
        const handle = startPlanAdapterEntry({
            workspace: fakeWorkspace(),
            options: { configChanged: optionsChanged.signal },
            callDbus: (_service, _path, _iface, method, payload, callback): void => {
                if (method === "NameHasOwner") {
                    callback(true);
                    return;
                }
                if (method === "GetNameOwner") {
                    callback(":1.7");
                    return;
                }
                if (method === "StartServiceByName") {
                    callback(1);
                    return;
                }
                mocks.dbusCalls.push({ method, payload });
                mocks.callbacks.push(callback);
            },
            scheduleOnce: (_delayMs, callback): (() => void) => {
                const timer = { callback, cancelled: false };
                mocks.timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: (message): void => {
                mocks.logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: (action, _text, sequence): boolean => {
                mocks.shortcuts.push({ action, sequence });
                return true;
            },
            readProfileFn: (): string => "cosmic",
            readWorkspaceModeFn: (): string => "per-output-local",
            readInnerGapFn: (): number => innerGap,
            readOuterGapFn: (): number => outerGap,
        });
        assert.ok(handle !== null);
        const shortcutCount = mocks.shortcuts.length;
        assert.ok(shortcutCount > 0);

        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1);
        assert.equal(mocks.dbusCalls[0]?.method, "DescribePlan");
        assert.deepEqual(domainOf(mocks.dbusCalls[0]?.payload as string), { gap: 8, outer_gap: 8 });
        const firstCorrelation = (JSON.parse(mocks.dbusCalls[0]?.payload as string) as { correlation_id: string })
            .correlation_id;
        mocks.callbacks[0]?.(
            JSON.stringify({ v: 1, correlation_id: firstCorrelation, outcome: "planned", desired_geometry: [] }),
        );

        const timersBeforeReload = mocks.timers.length;
        innerGap = 12;
        outerGap = 14;
        for (const fire of [...optionsChanged.handlers]) {
            fire();
        }
        assert.ok(
            mocks.logs.some((line) => line === "omnitiler:plan:config-reloaded stage=re-read-queued innerGap=12 outerGap=14 applied-unconfirmed"),
        );
        // requestResync is debounce-coalesced with the startup resync while its
        // timer is still pending in this harness, so timer growth is not
        // asserted here; the payload below is the behavioral proof.
        void timersBeforeReload;
        assert.equal(mocks.shortcuts.length, shortcutCount);

        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 2);
        assert.deepEqual(domainOf(mocks.dbusCalls[1]?.payload as string), { gap: 12, outer_gap: 14 });
        assert.equal(mocks.shortcuts.length, shortcutCount);
        handle?.stop();
    });

    it("ignores an unchanged configChanged without resync or shortcut work", () => {
        let innerGap = 8;
        let outerGap = 8;
        const optionsChanged = fakeSignal();
        const mocks: ReloadMocks = { dbusCalls: [], callbacks: [], timers: [], logs: [], shortcuts: [] };
        const handle = startPlanAdapterEntry({
            workspace: fakeWorkspace(),
            options: { configChanged: optionsChanged.signal },
            callDbus: (_service, _path, _iface, method, payload, callback): void => {
                if (method === "NameHasOwner") {
                    callback(true);
                    return;
                }
                if (method === "GetNameOwner") {
                    callback(":1.7");
                    return;
                }
                if (method === "StartServiceByName") {
                    callback(1);
                    return;
                }
                mocks.dbusCalls.push({ method, payload });
                mocks.callbacks.push(callback);
            },
            scheduleOnce: (_delayMs, callback): (() => void) => {
                const timer = { callback, cancelled: false };
                mocks.timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: (message): void => {
                mocks.logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: (action, _text, sequence): boolean => {
                mocks.shortcuts.push({ action, sequence });
                return true;
            },
            readProfileFn: (): string => "cosmic",
            readWorkspaceModeFn: (): string => "per-output-local",
            readInnerGapFn: (): number => innerGap,
            readOuterGapFn: (): number => outerGap,
        });
        assert.ok(handle !== null);
        const timersAtStart = mocks.timers.length;
        const shortcutCount = mocks.shortcuts.length;
        for (const fire of [...optionsChanged.handlers]) {
            fire();
        }
        assert.ok(!mocks.logs.some((line) => line.includes("omnitiler:plan:config-reloaded")));
        assert.equal(mocks.timers.length, timersAtStart);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.equal(mocks.shortcuts.length, shortcutCount);
        void innerGap;
        void outerGap;
        handle?.stop();
    });

    it("starts without an options signal surface and keeps ordinary reads stable", () => {
        const mocks: ReloadMocks = { dbusCalls: [], callbacks: [], timers: [], logs: [], shortcuts: [] };
        const handle = startPlanAdapterEntry({
            workspace: fakeWorkspace(),
            options: {},
            callDbus: (_service, _path, _iface, method, payload, callback): void => {
                if (method === "NameHasOwner") {
                    callback(true);
                    return;
                }
                if (method === "GetNameOwner") {
                    callback(":1.7");
                    return;
                }
                if (method === "StartServiceByName") {
                    callback(1);
                    return;
                }
                mocks.dbusCalls.push({ method, payload });
                mocks.callbacks.push(callback);
            },
            scheduleOnce: (_delayMs, callback): (() => void) => {
                const timer = { callback, cancelled: false };
                mocks.timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: (message): void => {
                mocks.logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: (action, _text, sequence): boolean => {
                mocks.shortcuts.push({ action, sequence });
                return true;
            },
            readProfileFn: (): string => "cosmic",
            readWorkspaceModeFn: (): string => "per-output-local",
            readInnerGapFn: (): number => 8,
            readOuterGapFn: (): number => 8,
        });
        assert.ok(handle !== null);
        handle?.requestFocus("left");
        assert.equal(mocks.dbusCalls.length, 1);
        assert.deepEqual(domainOf(mocks.dbusCalls[0]?.payload as string), { gap: 8, outer_gap: 8 });
        handle?.stop();
    });
});

describe("deliberate reload forwards gaps to subsequent workspace sends", () => {
    interface EchoHandles {
        readonly fireDesktops: (id: string) => void;
        readonly fireGeometry: (id: string) => void;
    }
    function twoDesktopWorkspace(echo?: { handles?: EchoHandles }): Record<string, unknown> {
        const output: Record<string, unknown> = { name: "out-1" };
        const ws1: Record<string, unknown> = { id: "ws-1", x11DesktopNumber: 1 };
        const ws2: Record<string, unknown> = { id: "ws-2", x11DesktopNumber: 2 };
        const currentByOutput = new Map<object, unknown>([[output, ws1]]);
        const desktopsById = new Map<string, Array<() => void>>();
        const geometryById = new Map<string, Array<() => void>>();
        if (echo !== undefined) {
            echo.handles = {
                fireDesktops: (id: string): void => {
                    for (const fire of [...(desktopsById.get(id) ?? [])]) {
                        fire();
                    }
                },
                fireGeometry: (id: string): void => {
                    for (const fire of [...(geometryById.get(id) ?? [])]) {
                        fire();
                    }
                },
            };
        }
        const makeWin = (id: string, desktop: unknown, x: number): Record<string, unknown> => {
            const desktopsChanged = fakeSignal();
            const frameGeometryChanged = fakeSignal();
            desktopsById.set(id, desktopsChanged.handlers);
            geometryById.set(id, frameGeometryChanged.handlers);
            const win: Record<string, unknown> = {
                normalWindow: true,
                managed: true,
                minimized: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
                internalId: id,
                resourceClass: "test-app",
                output,
                desktops: [desktop],
                frameGeometry: { x, y: 0, width: 100, height: 100 },
                frameGeometryChanged: frameGeometryChanged.signal,
                fullScreenChanged: fakeSignal().signal,
                maximizedChanged: fakeSignal().signal,
                desktopsChanged: desktopsChanged.signal,
            };
            win["setMaximize"] = (): void => {};
            return win;
        };
        const winA = makeWin("win-a", ws1, 0);
        const winB = makeWin("win-b", ws1, 100);
        const winT = makeWin("win-t", ws2, 0);
        void winT;
        const workspace: Record<string, unknown> = {
            screens: [output],
            desktops: [ws1, ws2],
            activeWindow: winA,
            activeScreen: output,
            windowList: (): unknown[] => [winA, winB, winT],
            currentDesktopForScreen: (out: unknown): unknown => currentByOutput.get(out as object) ?? null,
            currentDesktop: ws1,
            clientArea: (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowAdded: fakeSignal().signal,
            windowRemoved: fakeSignal().signal,
            windowActivated: fakeSignal().signal,
            screensChanged: fakeSignal().signal,
            currentDesktopChanged: fakeSignal().signal,
            desktopsChanged: fakeSignal().signal,
        };
        return workspace;
    }

    it("applies 8/8->12/14 to Plan and the next send source/target domains", () => {
        let innerGap = 8;
        let outerGap = 8;
        const optionsChanged = fakeSignal();
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }> = [];
        const logs: string[] = [];
        const handle = startPlanAdapterEntry({
            workspace: twoDesktopWorkspace(),
            options: { configChanged: optionsChanged.signal },
            callDbus: (_service, _path, _iface, method, payload, callback): void => {
                if (method === "NameHasOwner") {
                    callback(true);
                    return;
                }
                if (method === "GetNameOwner") {
                    callback(":1.7");
                    return;
                }
                if (method === "StartServiceByName") {
                    callback(1);
                    return;
                }
                dbusCalls.push({ method, payload });
                callbacks.push(callback);
            },
            scheduleOnce: (delayMs: number, callback: () => void): (() => void) => {
                const timer = { delayMs, callback, cancelled: false };
                timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: (message): void => {
                logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: (): boolean => true,
            readProfileFn: (): string => "cosmic",
            readWorkspaceModeFn: (): string => "per-output-local",
            readInnerGapFn: (): number => innerGap,
            readOuterGapFn: (): number => outerGap,
        });
        assert.ok(handle !== null);
        const fireDebounce = (): void => {
            const pending = [...timers];
            timers.length = 0;
            for (const timer of pending) {
                if (timer.cancelled) {
                    continue;
                }
                if (timer.delayMs === 120) {
                    timer.callback();
                } else {
                    timers.push(timer);
                }
            }
            for (let index = 0; index < dbusCalls.length; index += 1) {
                if (dbusCalls[index]?.method === "GetNameOwner") {
                    callbacks[index]?.(":1.7");
                }
            }
        };
        const planPayloads = (): Array<Record<string, unknown>> =>
            dbusCalls
                .filter((call) => call.method === "DescribePlan")
                .map((call) => JSON.parse(call.payload) as Record<string, unknown>)
                .filter((payload) => (payload["command"] as Record<string, unknown>)["op"] !== "send-to-workspace");
        const replied = new Set<number>();
        const settlePlans = (): void => {
            for (let round = 0; round < 8; round += 1) {
                fireDebounce();
                let progressed = false;
                for (let index = 0; index < dbusCalls.length; index += 1) {
                    if (replied.has(index)) {
                        continue;
                    }
                    const call = dbusCalls[index];
                    if (call === undefined || call.method !== "DescribePlan") {
                        continue;
                    }
                    let payload: Record<string, unknown>;
                    try {
                        payload = JSON.parse(call.payload) as Record<string, unknown>;
                    } catch {
                        continue;
                    }
                    if ((payload["command"] as Record<string, unknown>)["op"] === "send-to-workspace") {
                        continue;
                    }
                    const windows = payload["windows"] as Array<Record<string, unknown>>;
                    replied.add(index);
                    callbacks[index]?.(
                        JSON.stringify({
                            v: 1,
                            correlation_id: payload["correlation_id"],
                            outcome: "planned",
                            desired_geometry: windows.map((entry) => ({
                                window: entry["window"],
                                leaf: `leaf-${entry["window"] as string}`,
                                output: entry["output"],
                                workspace: entry["workspace"],
                                rect: entry["rect"],
                            })),
                        }),
                    );
                    progressed = true;
                }
                if (!progressed) {
                    break;
                }
            }
        };
        fireDebounce();
        assert.ok(planPayloads().length >= 1);
        const firstDomain = planPayloads()[0]?.["domain"] as Record<string, unknown>;
        assert.equal(firstDomain["gap"], 8);
        assert.equal(firstDomain["outer_gap"], 8);
        settlePlans();
        innerGap = 12;
        outerGap = 14;
        for (const fire of [...optionsChanged.handlers]) {
            fire();
        }
        assert.ok(logs.some((line) => line === "omnitiler:plan:config-reloaded stage=re-read-queued innerGap=12 outerGap=14 applied-unconfirmed"));
        settlePlans();
        const planAfter = planPayloads();
        const gapUpdate = planAfter.find(
            (payload) => (payload["command"] as Record<string, unknown>)["op"] === "update-gaps",
        ) as Record<string, unknown> | undefined;
        assert.ok(gapUpdate !== undefined, `update-gaps expected, got ${JSON.stringify(planAfter.map((p) => (p["command"] as Record<string, unknown>)["op"]))}`);
        assert.equal((gapUpdate["domain"] as Record<string, unknown>)["gap"], 12);
        assert.equal((gapUpdate["domain"] as Record<string, unknown>)["outer_gap"], 14);
        handle?.requestWorkspaceMove(2);
        const sendPayloads = dbusCalls
            .map((call) => {
                try {
                    return JSON.parse(call.payload) as Record<string, unknown>;
                } catch {
                    return null;
                }
            })
            .filter(
                (payload): payload is Record<string, unknown> =>
                    payload !== null && (payload["command"] as Record<string, unknown>)?.["op"] === "send-to-workspace",
            );
        assert.ok(sendPayloads.length >= 1, `send-to-workspace expected, got ${dbusCalls.length} calls`);
        const lastSend = sendPayloads[sendPayloads.length - 1] as Record<string, unknown>;
        assert.equal((lastSend["domain"] as Record<string, unknown>)["gap"], 12);
        assert.equal((lastSend["domain"] as Record<string, unknown>)["outer_gap"], 14);
        assert.equal((lastSend["target_domain"] as Record<string, unknown>)["gap"], 12);
        assert.equal((lastSend["target_domain"] as Record<string, unknown>)["outer_gap"], 14);
        handle?.stop();
    });

    it("logs queued applied-unconfirmed at signal and never claims applied, even after a commit", () => {
        let innerGap = 8;
        let outerGap = 8;
        const optionsChanged = fakeSignal();
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }> = [];
        const logs: string[] = [];
        const echo: { handles?: { fireDesktops: (id: string) => void; fireGeometry: (id: string) => void } } = {};
        const workspace = twoDesktopWorkspace(echo);
        const handle = startPlanAdapterEntry({
            workspace,
            options: { configChanged: optionsChanged.signal },
            callDbus: (_service, _path, _iface, method, payload, callback): void => {
                if (method === "NameHasOwner") {
                    callback(true);
                    return;
                }
                if (method === "GetNameOwner") {
                    callback(":1.7");
                    return;
                }
                if (method === "StartServiceByName") {
                    callback(1);
                    return;
                }
                dbusCalls.push({ method, payload });
                callbacks.push(callback);
            },
            scheduleOnce: (delayMs: number, callback: () => void): (() => void) => {
                const timer = { delayMs, callback, cancelled: false };
                timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: (message): void => {
                logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: (): boolean => true,
            readProfileFn: (): string => "cosmic",
            readWorkspaceModeFn: (): string => "per-output-local",
            readInnerGapFn: (): number => innerGap,
            readOuterGapFn: (): number => outerGap,
        });
        assert.ok(handle !== null);
        innerGap = 12;
        outerGap = 14;
        for (const fire of [...optionsChanged.handlers]) {
            fire();
        }
        assert.ok(
            logs.some(
                (line) => line === "omnitiler:plan:config-reloaded stage=re-read-queued innerGap=12 outerGap=14 applied-unconfirmed",
            ),
            logs.join("\n"),
        );
        // Queued is not applied: no committed flight has retained the pair yet.
        assert.ok(!logs.some((line) => line.includes("stage=applied")), logs.join("\n"));
        handle?.requestWorkspaceMove(2);
        const sendIndex = dbusCalls.findIndex((call) => {
            try {
                return ((JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)?.["op"] === "send-to-workspace";
            } catch {
                return false;
            }
        });
        assert.ok(sendIndex >= 0);
        const sendPayload = JSON.parse(dbusCalls[sendIndex]?.payload as string) as Record<string, unknown>;
        const correlation = sendPayload["correlation_id"] as string;
        assert.ok(typeof correlation === "string" && correlation.length > 0);
        // Reload behavior: the queued pair is frozen into the next send.
        assert.equal((sendPayload["domain"] as Record<string, unknown>)["gap"], 12);
        assert.equal((sendPayload["domain"] as Record<string, unknown>)["outer_gap"], 14);
        assert.equal((sendPayload["target_domain"] as Record<string, unknown>)["gap"], 12);
        assert.equal((sendPayload["target_domain"] as Record<string, unknown>)["outer_gap"], 14);
        const geometry = (sendPayload["windows"] as Array<Record<string, unknown>>).concat(
            sendPayload["target_windows"] as Array<Record<string, unknown>>,
        );
        callbacks[sendIndex]?.(
            JSON.stringify({
                v: 1,
                correlation_id: correlation,
                outcome: "planned",
                kind: "send-to-workspace",
                base_revision: 0,
                detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
                desired_geometry: geometry.map((entry) => ({
                    window: entry["window"],
                    leaf: `leaf-${entry["window"] as string}`,
                    output: entry["output"],
                    workspace: "ws-2",
                    rect: entry["rect"],
                })),
                desired_focus: { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-a" },
                preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
                operation: {
                    op: "move-tiled",
                    window: "win-a",
                    leaf: "leaf-win-a",
                    source_output: "out-1",
                    source_workspace: "ws-1",
                    target_output: "out-1",
                    target_workspace: "ws-2",
                },
            }),
        );
        // Immediate-commit truth: one planned reply actuates synchronously.
        // There is no ack/verify wire protocol on DescribePlan.
        const opsAfterSend = dbusCalls.slice(sendIndex + 1).map((call) => {
            try {
                return ((JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)?.["op"] as string;
            } catch {
                return "?";
            }
        });
        assert.ok(!opsAfterSend.includes("send-to-workspace-ack"), `no ack protocol, got ${opsAfterSend.join(",")}`);
        assert.ok(!opsAfterSend.includes("send-to-workspace-verify"), `no verify protocol, got ${opsAfterSend.join(",")}`);
        assert.ok(
            logs.some((line) => line.includes(`correlation=${correlation}`) && line.includes("event=plan") && line.includes("outcome=planned")),
            logs.join("\n"),
        );
        assert.ok(
            logs.some((line) => line.includes(`correlation=${correlation}`) && line.includes("event=write") && line.includes("outcome=applied")),
            logs.join("\n"),
        );
        // The planned topology commits to native membership synchronously.
        const wins = (workspace["windowList"] as () => Array<Record<string, unknown>>)();
        const mover = wins.find((entry) => entry["internalId"] === "win-a");
        assert.ok(mover !== undefined);
        assert.ok(
            ((mover["desktops"] as Array<Record<string, unknown>>).some((entry) => entry["id"] === "ws-2")),
            JSON.stringify((mover["desktops"] as Array<Record<string, unknown>>).map((entry) => entry["id"])),
        );
        // Settlement forces a complete source AND target refresh through the
        // single-flight Plan chain. Drive the debounce; owner activation is
        // synchronous in this harness.
        const planBefore = dbusCalls.length;
        for (let round = 0; round < 4; round += 1) {
            const pending = [...timers];
            timers.length = 0;
            for (const timer of pending) {
                if (timer.cancelled) {
                    continue;
                }
                if (timer.delayMs === 120) {
                    timer.callback();
                } else {
                    timers.push(timer);
                }
            }
            if (dbusCalls.length > planBefore) {
                break;
            }
        }
        const refreshOps = dbusCalls.slice(planBefore).map((call) => {
            try {
                return ((JSON.parse(call.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)?.["op"] as string;
            } catch {
                return "?";
            }
        });
        assert.ok(
            refreshOps.some((op) => op === "admit" || op === "reconcile"),
            `forced reconcile DescribePlan expected, got ${refreshOps.join(",")}`,
        );
        void echo;
        // Provenance: the send committed with the queued pair, but the
        // update-gaps plan flight runs the separate planner channel, so no
        // applied claim may follow. The queued line stays applied-unconfirmed.
        assert.ok(!logs.some((line) => line.includes("stage=applied")), logs.join("\n"));
        assert.ok(
            logs.some(
                (line) => line === "omnitiler:plan:config-reloaded stage=re-read-queued innerGap=12 outerGap=14 applied-unconfirmed",
            ),
            logs.join("\n"),
        );
        handle?.stop();
    });

    it("logs restart-required for startup key drift without adopting it", () => {
        let workspaceMode = "per-output-local";
        const optionsChanged = fakeSignal();
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }> = [];
        const logs: string[] = [];
        const handle = startPlanAdapterEntry({
            workspace: twoDesktopWorkspace(),
            options: { configChanged: optionsChanged.signal },
            callDbus: (_service, _path, _iface, method, payload, callback): void => {
                if (method === "NameHasOwner") {
                    callback(true);
                    return;
                }
                if (method === "GetNameOwner") {
                    callback(":1.7");
                    return;
                }
                if (method === "StartServiceByName") {
                    callback(1);
                    return;
                }
                dbusCalls.push({ method, payload });
                callbacks.push(callback);
            },
            scheduleOnce: (delayMs: number, callback: () => void): (() => void) => {
                const timer = { delayMs, callback, cancelled: false };
                timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: (message): void => {
                logs.push(message);
            },
            owner: "owner-1",
            generation: "gen-1",
            registerShortcutFn: (): boolean => true,
            readProfileFn: (): string => "cosmic",
            readWorkspaceModeFn: (): string => workspaceMode,
            readInnerGapFn: (): number => 8,
            readOuterGapFn: (): number => 8,
        });
        assert.ok(handle !== null);
        workspaceMode = "shared";
        for (const fire of [...optionsChanged.handlers]) {
            fire();
        }
        assert.ok(
            logs.some(
                (line) => line === "omnitiler:plan:config-reloaded stage=restart-required keys=workspaceMode",
            ),
            logs.join("\n"),
        );
        // Gaps unchanged: nothing queued and nothing applied.
        assert.ok(!logs.some((line) => line.includes("stage=re-read-queued")), logs.join("\n"));
        assert.ok(!logs.some((line) => line.includes("stage=applied")), logs.join("\n"));
        void dbusCalls;
        void callbacks;
        void timers;
        handle?.stop();
    });
});
